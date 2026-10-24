// Package rect_pack is a skyline rectangle packer.
//
// This is a pure Rust rewrite of stb_rect_pack.h (Skyline Bottom-Left /
// Best-Fit skyline algorithm, v1.01) by Sean Barrett, released under the
// MIT / public-domain licenses. The algorithm and its linked-list node
// handling are transliterated faithfully from the original C; the API has
// been reshaped into a Rust crate.
//
//   Credit:
//     Library  - Sean Barrett
//     Minor    - Martins Mozeiko, github:IntellectualKitty
//     Bugfixes - Jeremy Jaussaud, Fabian Giesen
//     Port     - this Rust port by the dssodinlib maintainers
//
// The Skyline algorithm
//
// The bin is imagined as a city skyline viewed from the left: a sequence of
// horizontal "steps" from y=0 on the left to the right edge. Each step is a
// skyline node: {x, y, next} where x is where the step starts and y is its
// height. The sentinel node at the right edge has y = INFINITY so the
// iterator always terminates.
//
// To place a rectangle of width w at x0 the algorithm walks the skyline
// from x0 to x0+w, finds the maximum y in that range (the rectangle's
// bottom edge), and records it. It then rewrites the skyline to include a
// new step at x0 with height = max_y + h (the rectangle's top), pruning
// any existing steps that fall entirely under the rectangle.
//
// The "Bottom-Left" heuristic (Skyline_default) picks the placement with
// the lowest y over all valid x positions; the "Best-Fit" heuristic
// (Skyline_BF_sortHeight) additionally prefers placements with the least
// wasted area beneath the rectangle, and tries right-aligning each
// rectangle against every skyline node (at the cost of ~2x time).
//
// Node pool
//
// The skyline nodes live in a flat Vec<Node> of length >= max_rects + 2.
// Nodes 0 and 1 are the active-chain sentinel pair (see below). Nodes
// 2 ..< len are initially threaded into a free list via their .next field.
// When a rectangle is placed, one node is popped from the free list, its x
// and y are set to the rectangle's top edge, and it is spliced into the
// active chain. When a skyline step becomes redundant it is recycled back
// into the free list. The free list never grows; it can only shrink, so
// capacity is bounded by the initial allocation.
//
// Two sentinel nodes (extra[2] in the original C code, nodes[0..1] here)
// are required so the optimal user-node-count is `width` rather than
// `width + 2`.  Node 0 covers the full bin width at y=0 (the initial
// flat skyline). Node 1 is the right-edge terminator with y = INFINITY;
// its x position is stored but we never need to scan past it, so it
// also serves as the proof the scan loop always terminates.
//
// The link indirection trick
//
// The active chain is singly-linked, but we frequently need to rewrite
// the *owner* of a link (e.g. the active-head or a node's .next) without
// knowing whether the owner is the global head or a previous node.  To
// avoid special-casing, we pass a "link owner" index: the special value
// HEAD_LINK means "the packer's active_head field", and any other value
// means "the .next field of nodes[link]".  link_next / link_set perform
// this indirection so the rest of the code stays clean.
//
// Width quantization
//
// Each placed rectangle consumes at most one skyline node. The skyline
// for a bin of width W can grow to at most W+1 nodes, but the free list
// only has `free_count` nodes available.  To guarantee the algorithm
// never runs out of temporary storage, request widths are quantized to
// multiples of `align`, which is ceil(width / free_count).  Setting
// align = 1 (via setup_allow_out_of_mem) disables quantization for
// tighter packing at the risk of running out of nodes.
//
// Typical usage:
//
//	let mut packer = rect_pack::Packer::default();
//	packer.init(atlas_width, atlas_height, sizes.len(), rect_pack::Heuristic::Skyline_default);
//
//	let (results, all_packed) = packer.pack(&sizes, 1, true);
//	for (r, request) in results.iter().zip(&sizes) {
//	    if !r.was_packed {
//	        continue; // did not fit into the bin
//	    }
//	    draw_at(r.x, r.y, request.w, request.h);
//	}
//
// Results are index-aligned with requests: results[i] describes requests[i].
// Coordinates are pixel offsets in the target bin; each packed rectangle
// occupies [x, x+w) x [y, y+h).

/// Private sentinel: a skyline height that can never occur in practice.
const INFINITY: i32 = 1 << 30;

/// Private sentinel link value that terminates a chain.
const NIL: i32 = -1;

// HEAD_LINK is a sentinel link-owner index meaning "the packer's active
// head" rather than a specific node.  It lets link_next/link_set avoid
// special-casing the head-of-list write.
const HEAD_LINK: i32 = -2;

// Heuristic selects which packing strategy [pack] uses.
//
// Skyline_default (Bottom-Left) picks the placement with the lowest y over
// all valid x positions.  It is fast (single scan per candidate) and works
// well for most atlas layouts.
//
// Skyline_BF_sortHeight (Best-Fit) additionally minimises the wasted area
// beneath the placed rectangle and also tries aligning the rectangle's
// right edge to each skyline node.  This usually packs tighter but takes
// about twice as long because of the extra right-alignment pass.
//
// Different heuristics produce better/worse results for different data
// sets; Skyline_default is a sensible, faster starting point.
//
// Note: `Skyline_default == Skyline_BL_sortHeight` (both 0), so this is a
// newtype rather than an enum to preserve the original's aliasing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Heuristic(i32);

#[allow(non_upper_case_globals)]
impl Heuristic {
    pub const Skyline_default: Heuristic = Heuristic(0);
    pub const Skyline_BL_sortHeight: Heuristic = Heuristic(0);
    pub const Skyline_BF_sortHeight: Heuristic = Heuristic(1);
}

// Request is the size of a single rectangle to pack. Requests are not
// mutated; [pack] copies them into internal scratch before sorting.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Request {
    pub w: i32,
    pub h: i32,
}

// Result is the placement outcome for one request, index-aligned with it.
//
// If was_packed is false the rectangle did not fit and all fields except
// w/h are zeroed.  Otherwise the rectangle content occupies
// [x, x+w) x [y, y+h) inside the bin; the padding passed to [pack] is not
// part of the rectangle itself — it only shifts x/y and keeps gaps between
// neighboring contents, so the padded slot spans [x-padding, x+w+padding)
// x [y-padding, y+h+padding).
//
// Coordinates are bottom-left in Cartesian terms (y increases upward), or
// top-left if you imagine y increasing downward — the packer is
// orientation-agnostic.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Result {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub was_packed: bool,
}

// Node is a single step in the skyline. x is the horizontal position where
// this step starts, y is its height, and next is the index of the next node
// (NIL for the right-edge sentinel).  Nodes live in a flat pool owned by
// [Packer]; the pool is partitioned at runtime into the active skyline chain
// (linked through active_head) and a free list (linked through free_head).
// Callers never inspect or construct Node values directly.
#[derive(Clone, Copy, Debug, Default)]
struct Node {
    x: i32,
    y: i32,
    next: i32,
}

// Packer places rectangles into a single width x height bin using the
// skyline heuristic.
//
// Internally it maintains an active-chain of skyline nodes (singly-linked
// through .next, head stored in active_head) plus a free-list of unused
// nodes (head in free_head).  During [init] both chains are carved from
// a single Vec<Node> pool: nodes 0 and 1 are the sentinel pair (the initial
// flat skyline), and the remainder are threaded into the free list.
//
// Storage is allocated once during [init]: enough skyline nodes for up to
// max_rects rectangles plus at least one node per column of the bin so that
// request widths are never quantized (see [setup_allow_out_of_mem] for the
// alternative). Nothing else is allocated except the results returned by
// [pack] and transient scratch inside it, so a Packer can be reused for many
// bins by calling [init] again.
#[derive(Clone, Debug, Default)]
pub struct Packer {
    nodes: Vec<Node>, // skyline node pool: active chain + free chain interleaved
    width: i32,
    height: i32,
    align: i32, // quantization to keep free nodes sufficient (see setup_allow_out_of_mem)
    initialized: bool,
    heuristic: Heuristic,
    active_head: i32, // index of the first active skyline node
    free_head: i32,   // index of the first free node
}

// free_count returns the number of nodes available for packing.  The pool
// always has two sentinel nodes that can never be consumed, so the usable
// count is len(nodes) - 2.
impl Packer {
    /// Width of the current bin.
    pub fn width(&self) -> i32 {
        self.width
    }

    /// Height of the current bin.
    pub fn height(&self) -> i32 {
        self.height
    }

    fn free_count(&self) -> i32 {
        (self.nodes.len() as i32 - 2).max(0)
    }

    // ---------------------------------------------------------------------------
    // Lifecycle.
    // ---------------------------------------------------------------------------

    // init prepares packer for packing into a fresh width x height bin that can
    // hold at most max_rects rectangles, resetting any previous state.
    //
    // The heuristic argument selects the packing heuristic subsequent [pack]
    // calls use (see [Heuristic]).
    //
    // It must be called before every new bin; calling [pack] again afterwards
    // without re-initializing continues filling the current bin instead.
    //
    // The node pool is sized to max(max_rects, width) + 2, which guarantees
    // that even in the worst case (every rectangle is one pixel wide) the
    // skyline never needs more nodes than are available.  If max_rects <= 0
    // the packer is initialized to an empty state that rejects all placements.
    pub fn init(&mut self, width: i32, height: i32, max_rects: i32, heuristic: Heuristic) {
        // Reset the struct to zero/defaults. Dropping the old node pool
        // releases it, so re-init for a new bin must not leak.
        self.nodes = Vec::new();
        self.width = width;
        self.height = height;
        self.align = 0;
        self.initialized = false;
        self.heuristic = Heuristic::default();
        self.active_head = NIL;
        self.free_head = NIL;
        // A zero or negative max_rects creates an empty packer that rejects
        // every placement — useful for dry-run sizing.
        if max_rects <= 0 {
            return;
        }
        // Allocate the node pool: max(max_rects, width) + 2 guarantees at
        // least one free node per column even if every rectangle is one pixel
        // wide.  The +2 accounts for the two sentinel nodes.
        self.nodes = vec![Node::default(); (max_rects.max(width) + 2) as usize];
        self.init_target(width, height);
        self.setup_heuristic(heuristic);
    }

    // destroy releases all storage owned by the packer. Safe to call on an
    // already destroyed or never initialized packer.
    pub fn destroy(&mut self) {
        self.nodes = Vec::new();
        *self = Packer::default();
    }

    // ---------------------------------------------------------------------------
    // Configuration.
    // ---------------------------------------------------------------------------

    // setup_allow_out_of_mem controls width quantization.  When allow is false
    // (the default) request widths are rounded up to multiples of
    // ceil(width / free_count) so the skyline never grows beyond the pool.
    // This guarantees the algorithm never runs out of nodes, at the cost of
    // slightly wider packed slots.
    //
    // When allow is true, widths are used exactly (align = 1).  This gives
    // tighter packing but the algorithm can fail with an out-of-nodes error
    // even though the rectangles physically fit in the bin.
    //
    // The original C comment: "A smarter approach would be to only quantize
    // once we've hit OOM, then we could get rid of this parameter."
    #[allow(dead_code)]
    fn setup_allow_out_of_mem(&mut self, allow: bool) {
        if allow {
            // no quantization: use exact widths (tighter packing, may run out of nodes)
            self.align = 1;
        } else {
            // quantize widths so that free_count * align >= width, guaranteeing
            // the skyline never needs more nodes than the pool provides
            let free = self.free_count().max(1);
            self.align = (self.width + free - 1) / free;
        }
    }

    // setup_heuristic selects the packing heuristic used by subsequent [pack]
    // calls. [init] already accepts a heuristic, so this is only needed to
    // switch between packs without re-initializing.
    #[allow(dead_code)]
    fn setup_heuristic(&mut self, heuristic: Heuristic) {
        if !self.initialized {
            panic!("rect_pack: init_target not called");
        }
        if heuristic == Heuristic::Skyline_default || heuristic == Heuristic::Skyline_BF_sortHeight
        {
            self.heuristic = heuristic;
        } else {
            panic!("rect_pack: invalid heuristic");
        }
    }

    // init_target initializes the skyline around an externally provided node
    // array, exactly as the underlying STB function does.  [init] wraps this
    // and owns the storage; you only need this if you manage the node memory
    // yourself.
    //
    // The initial skyline is a single flat step from x=0 to x=width at y=0
    // (node 0), terminated by a sentinel at x=width with y=INFINITY (node 1).
    // All remaining nodes in the array are threaded into the free list.
    fn init_target(&mut self, width: i32, height: i32) {
        let n = self.nodes.len();
        assert!(n >= 2);

        // Thread nodes 2 ..< n-1 into a singly-linked free list.
        // Each node's .next points to the next index; the last node terminates
        // with NIL.  This gives us n-2 free nodes to consume during packing.
        for i in 2..n - 1 {
            self.nodes[i].next = i as i32 + 1;
        }
        self.nodes[n - 1].next = NIL;

        self.initialized = true;
        self.heuristic = Heuristic::Skyline_default;
        // free_head starts at node 2 (first non-sentinel); if only 2 nodes exist
        // there are no free nodes to place into, so mark the list empty.
        self.free_head = if n > 2 { 2 } else { NIL };
        self.width = width;
        self.height = height;

        // The initial skyline is a single horizontal step from x=0 to x=width
        // at height y=0, followed by the right-edge sentinel at y=INFINITY.
        // Node 0 (left step) points at node 1 (sentinel), which terminates
        // with NIL — the active chain is 0 -> 1 -> NIL.
        self.nodes[0] = Node {
            x: 0,
            y: 0,
            next: 1,
        };
        self.nodes[1] = Node {
            x: width,
            y: INFINITY,
            next: NIL,
        };
        self.active_head = 0;

        self.setup_allow_out_of_mem(false);
    }

    // ---------------------------------------------------------------------------
    // Skyline linked-list helpers.
    // ---------------------------------------------------------------------------

    // link_next reads the node index stored at the "link owner" link.  The
    // special value HEAD_LINK means "the packer's active_head field"; any other
    // value is an index into the nodes array whose .next field is read.
    //
    // This indirection exists because the active chain is singly-linked and
    // we frequently need to rewrite the predecessor of a node without knowing
    // whether the predecessor is a previous node or the list head.  The C
    // original uses stbrp_node** (pointer-to-pointer) for this; we use an
    // integer tag that encodes the same idea without pointer aliasing.
    fn link_next(&self, link: i32) -> i32 {
        if link == HEAD_LINK {
            self.active_head
        } else {
            self.nodes[link as usize].next
        }
    }

    // link_set stores value into the .next field of the link owner link.
    // When link is HEAD_LINK it writes the packer's active_head.
    fn link_set(&mut self, link: i32, value: i32) {
        if link == HEAD_LINK {
            self.active_head = value;
        } else {
            self.nodes[link as usize].next = value;
        }
    }

    // skyline_find_min_y computes two things in a single left-to-right scan:
    //
    //  1. min_y — the maximum skyline height over the horizontal range
    //     [x0, x0+width).  This is the lowest y-coordinate at which a
    //     rectangle of the given width can be placed starting at x0 without
    //     overlapping the existing skyline.
    //
    //  2. waste_area — the total area between the skyline and min_y over that
    //     same range.  This is the "dead space" that the new rectangle will
    //     leave beneath it.  The Best-Fit heuristic uses waste_area to prefer
    //     placements with less dead space when two candidates share the same y.
    //
    // The scan walks the active chain starting from node 'first'.  Each
    // iteration handles one skyline segment.  When a node's y exceeds the
    // current min_y the minimum is raised and the accumulated visited_width is
    // used to charge waste for the lower region we already passed.  When a
    // node's y is below the current min_y, waste is added for the strip
    // between that node's y and min_y.  The first node may straddle x0, so
    // its contribution is clamped to [x0, node.next.x).
    fn skyline_find_min_y(&self, first: i32, x0: i32, width: i32) -> (i32, i32) {
        let x1 = x0 + width;
        let mut node = first;
        let mut visited_width = 0;
        let mut min_y = 0;
        let mut waste_area = 0;

        // Walk the skyline from 'first' rightward until we pass x1.
        // At each step we update min_y (the highest y in the range) and
        // waste_area (the total dead space between the skyline and min_y).
        while self.nodes[node as usize].x < x1 {
            let nxt = self.nodes[node as usize].next;
            if self.nodes[node as usize].y > min_y {
                // This segment is higher than any we've seen: raise min_y.
                // Everything we already visited was below this new min_y, so
                // charge waste for the gap between old min_y and new min_y
                // across all previously visited width.
                waste_area += visited_width * (self.nodes[node as usize].y - min_y);
                min_y = self.nodes[node as usize].y;
                // Accumulate the width of this segment.  On the first iteration
                // the segment may start before x0, so clamp to [x0, nxt.x).
                if self.nodes[node as usize].x < x0 {
                    visited_width += self.nodes[nxt as usize].x - x0;
                } else {
                    visited_width += self.nodes[nxt as usize].x - self.nodes[node as usize].x;
                }
            } else {
                // This segment is at or below min_y: charge waste for the strip
                // between this segment's y and min_y, but only for the width
                // that falls inside our [x0, x1) range and hasn't been counted yet.
                let under_width = (self.nodes[nxt as usize].x - self.nodes[node as usize].x)
                    .min(width - visited_width);
                waste_area += under_width * (min_y - self.nodes[node as usize].y);
                visited_width += under_width;
            }
            node = self.nodes[node as usize].next;
        }
        (min_y, waste_area)
    }

    // ---------------------------------------------------------------------------
    // Skyline placement.
    // ---------------------------------------------------------------------------

    // find_result is the outcome of searching for the best placement of a single
    // rectangle.  x/y give the placement coordinates (or INFINITY if no fit
    // was found) and link is the link-owner index whose .next must be
    // rewritten when the rectangle is spliced into the active chain.
    fn find_best_pos(&self, req_width: i32, height: i32) -> find_result {
        let mut fr = find_result {
            x: 0,
            y: 0,
            link: NIL,
        };

        // Quantize the request width up to the next multiple of p.align so
        // the skyline never grows beyond the pool capacity.
        let mut width = req_width + self.align - 1;
        width -= width % self.align;

        // Quick rejection: impossible dimensions can never fit.
        if width > self.width || height > self.height {
            return fr;
        }

        let mut best_waste = INFINITY;
        let mut best_y = INFINITY;
        let mut best = NIL;

        // ---- Pass 1: left-aligned candidates (both heuristics) ----
        // Walk the active chain.  For each node, test placing the rectangle's
        // left edge at that node's x.  Call skyline_find_min_y to find the
        // lowest y the rectangle can sit at from that x position.
        let mut node = self.active_head;
        let mut prev = HEAD_LINK; // prev tracks the link-owner of 'node' for later splice
        let heuristic = self.heuristic;
        while node != NIL && self.nodes[node as usize].x + width <= self.width {
            let (y, waste) = self.skyline_find_min_y(node, self.nodes[node as usize].x, width);
            if heuristic == Heuristic::Skyline_BF_sortHeight {
                // Best-Fit: reject placements that would exceed the bin height,
                // then prefer lowest y; break ties with least waste.
                if y + height <= self.height && (y < best_y || (y == best_y && waste < best_waste))
                {
                    best_y = y;
                    best_waste = waste;
                    best = prev;
                }
            } else {
                // Bottom-Left: simply prefer the lowest y.
                if y < best_y {
                    best_y = y;
                    best = prev;
                }
            }
            prev = node;
            node = self.nodes[node as usize].next;
        }

        // Resolve the x-coordinate of the winning left-aligned candidate.
        // If no candidate was found (best == NIL), x defaults to 0.
        let mut best_x = if best == NIL {
            0
        } else {
            self.nodes[self.link_next(best) as usize].x
        };

        // ---- Pass 2: right-aligned candidates (Best-Fit only) ----
        // For each skyline node whose x is >= width, try placing the rectangle
        // with its right edge aligned to that x.  This catches cases where
        // right-aligning reduces waste that left-alignment would leave.
        if heuristic == Heuristic::Skyline_BF_sortHeight {
            let mut tail = self.active_head;
            node = self.active_head;
            prev = HEAD_LINK;
            // Advance tail until its x >= width (the first node that can serve
            // as a right-edge anchor for a rectangle of this width).
            while tail != NIL && self.nodes[tail as usize].x < width {
                tail = self.nodes[tail as usize].next;
            }
            // For each admissible tail, compute the candidate xpos and evaluate
            // it just like pass 1.
            while tail != NIL {
                let xpos = self.nodes[tail as usize].x - width;
                // Advance node/prev so that node is the first skyline node at
                // or past xpos — the starting point for skyline_find_min_y.
                while node != NIL && self.nodes[self.link_next(node) as usize].x <= xpos {
                    prev = node;
                    node = self.link_next(node);
                }
                let (y, waste) = self.skyline_find_min_y(node, xpos, width);
                if y + height <= self.height && y <= best_y {
                    // Prefer lower y; break ties with less waste, then with
                    // the leftmost xpos for determinism.
                    if y < best_y || waste < best_waste || (waste == best_waste && xpos < best_x) {
                        best_x = xpos;
                        best_y = y;
                        best_waste = waste;
                        best = prev;
                    }
                }
                tail = self.nodes[tail as usize].next;
            }
        }

        fr = find_result {
            x: best_x,
            y: best_y,
            link: best,
        };
        fr
    }

    // pack_single places a single rectangle by:
    //
    //  1. Finding the best position via find_best_pos.
    //
    //  2. Bail if the search failed, the rectangle exceeds the bin height, or
    //     the free list is empty (no nodes left to represent the new skyline
    //     step).
    //
    //  3. Pop a node from the free list and set its x/y to the rectangle's
    //     top-left corner (x, y + height).  This new node represents the
    //     skyline step *above* the placed rectangle.
    //
    //  4. Splice the new node into the active chain at the correct position:
    //     - If the predecessor node ends before the rectangle's left edge,
    //       keep the predecessor and start unlinking from its successor.
    //     - Otherwise rewrite the predecessor's link to point at the new node.
    //
    //  5. Walk forward from the splice point, recycling any intermediate nodes
    //     whose x falls inside [res.x, res.x + width).  These nodes represent
    //     skyline steps that are now completely covered by the rectangle and
    //     are no longer needed.
    //
    //  6. Stitch the remaining chain back: the new node's .next points to the
    //     first surviving node.  If that survivor's x is less than the
    //     rectangle's right edge (it straddles the boundary), advance its x
    //     to res.x + width so the skyline stays non-decreasing.
    fn pack_single(&mut self, width: i32, height: i32) -> Option<(i32, i32)> {
        let res = self.find_best_pos(width, height);

        // Bail if:
        //  1. The search found no candidate (res.link == NIL).
        //  2. The candidate y + height exceeds the bin top.
        //  3. The free list is empty — no node available to represent the new
        //     skyline step above the placed rectangle.
        if res.link == NIL || res.y + height > self.height || self.free_head == NIL {
            return None;
        }

        // Pop a node from the free list and initialise it as the new skyline
        // step at the rectangle's top edge.  This is the node that will be
        // spliced into the active chain in the next steps.
        let node = self.free_head;
        self.nodes[node as usize].x = res.x;
        self.nodes[node as usize].y = res.y + height;
        self.free_head = self.link_next(node);

        // Splice the new node into the active chain after 'res.link'.
        // 'cur' is the first node whose position we need to decide about.
        let mut cur = self.link_next(res.link);
        if self.nodes[cur as usize].x < res.x {
            // The current node starts before the rectangle's left edge — it
            // is not covered by the rectangle, so keep it.  Rewire its .next
            // to point at the new node, then continue with cur's successor.
            let nxt = self.nodes[cur as usize].next;
            self.nodes[cur as usize].next = node;
            cur = nxt;
        } else {
            // The current node starts at or after the rectangle's left edge —
            // the new node takes its place.
            self.link_set(res.link, node);
        }

        // Walk forward and recycle any intermediate nodes that fall entirely
        // under the rectangle (their x <= res.x + width).  These skyline steps
        // are now invisible — the rectangle covers them — so they go back to
        // the free list for future placements.
        while cur != NIL
            && self.link_next(cur) != NIL
            && self.nodes[self.link_next(cur) as usize].x <= res.x + width
        {
            let nxt = self.link_next(cur);
            self.link_set(cur, self.free_head);
            self.free_head = cur;
            cur = nxt;
        }

        // Stitch the remaining chain: the new node points at the first
        // surviving node 'cur'.
        self.nodes[node as usize].next = cur;
        // If 'cur' straddles the rectangle's right edge (cur.x < res.x + width),
        // advance its x so the skyline stays non-decreasing — the covered
        // portion of this step is consumed by the rectangle.
        if cur != NIL && self.nodes[cur as usize].x < res.x + width {
            self.nodes[cur as usize].x = res.x + width;
        }

        Some((res.x, res.y))
    }

    // ---------------------------------------------------------------------------
    // Public packing API.
    // ---------------------------------------------------------------------------

    // pack places every requested rectangle into the current bin and returns
    // placements index-aligned with requests. The second return reports
    // whether every request was packed.
    //
    // The algorithm:
    //  1. Copy requests into scratch (Tagged), adding padding on each side and
    //     recording original indices so the caller's slice is never reordered.
    //  2. Sort the scratch tallest-first (deterministic tie-break by index) so
    //     large rectangles are placed first when the skyline is most open.
    //  3. Place each rectangle via pack_single, which modifies the skyline
    //     node pool but nothing else.
    //  4. Results are written back index-aligned with the original requests.
    //
    // Calling pack again on the same packer continues filling the current bin;
    // call [init] first to start a new bin.  Requests are copied into scratch
    // before packing, so the caller's slice is never reordered and stays safe
    // to read from other threads.
    //
    // At most free_count rectangles can be packed per call; extra requests
    // beyond that capacity stay unpacked rather than overflowing the skyline
    // storage.  Check was_packed per result as well.
    //
    // padding grows every rectangle by padding pixels on each side before
    // packing (to w+2*padding by h+2*padding) so packed rectangles never touch;
    // this prevents edge bleeding when a shared sheet is sampled with linear
    // filtering.  It defaults to 1.  Results are padding-free: x/y point at the
    // content (slot origin + padding) and w/h mirror the request size.
    pub fn pack(
        &mut self,
        requests: &[Request],
        padding: i32,
        should_sort: bool,
    ) -> (Vec<Result>, bool) {
        // Allocate the result slice up front — one slot per request, zeroed.
        // Results that couldn't be packed keep was_packed = false.
        let mut results = vec![Result::default(); requests.len()];
        let mut all_packed = true;

        // Clamp the number of rectangles we'll attempt to pack to the number
        // of free nodes available.  Each placement consumes at most one node;
        // exceeding this would corrupt the skyline.
        let n = (requests.len() as i32).min(self.free_count());
        if n <= 0 {
            all_packed = requests.is_empty();
            return (results, all_packed);
        }
        let n = n as usize;

        // Copy the first n requests into scratch, adding padding on each side
        // (w+2*padding, h+2*padding) and recording the original index.  This
        // preserves the caller's slice and makes the sort stable w.r.t.
        // request order.
        let mut tagged: Vec<Tagged> = (0..n)
            .map(|i| Tagged {
                w: requests[i].w + padding * 2,
                h: requests[i].h + padding * 2,
                orig: i,
            })
            .collect();

        // Sort tallest-first so the largest rectangles are placed first while
        // the skyline is most open and has the most room.  Tie-breakers
        // (width, then original index) keep the layout deterministic.
        if should_sort {
            tagged.sort_by(|a, b| {
                if a.h != b.h {
                    b.h.cmp(&a.h)
                } else if a.w != b.w {
                    b.w.cmp(&a.w)
                } else {
                    a.orig.cmp(&b.orig)
                }
            });
        }

        // Place each rectangle in sorted order, writing results back at the
        // original index so results[i] corresponds to requests[i].
        for tag in tagged {
            let result = &mut results[tag.orig];
            result.w = requests[tag.orig].w;
            result.h = requests[tag.orig].h;

            if request_is_empty(requests[tag.orig]) {
                // Zero-extent rectangle occupies no space — mark it packed
                // without touching the skyline.
                result.was_packed = true;
                continue;
            }

            // Pack the padded rectangle, then shift the reported coordinates
            // by +padding so the result points at the content origin, not the
            // padded slot origin.
            if let Some((x, y)) = self.pack_single(tag.w, tag.h) {
                result.x = x + padding;
                result.y = y + padding;
                result.was_packed = true;
            } else {
                result.was_packed = false;
                all_packed = false;
            }
        }
        (results, all_packed)
    }
}

// find_result is the outcome of searching for the best placement of a single
// rectangle.  x/y give the placement coordinates (or INFINITY if no fit
// was found) and link is the link-owner index whose .next must be
// rewritten when the rectangle is spliced into the active chain.
#[allow(non_camel_case_types)]
#[derive(Clone, Copy, Debug, Default)]
struct find_result {
    x: i32,
    y: i32,
    link: i32,
}

// request_is_empty reports whether a request has zero extent on either axis.
// Empty rectangles need no space and are marked packed immediately.
#[inline(always)]
fn request_is_empty(r: Request) -> bool {
    r.w == 0 || r.h == 0
}

// Tagged is a padded request plus its original request index.  It is the
// internal sorting/packing unit so the caller's request slice is never
// mutated and so the sort is stable with respect to request order.
#[derive(Clone, Copy, Debug, Default)]
struct Tagged {
    w: i32,
    h: i32,
    orig: usize,
}
