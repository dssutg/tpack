//! guillotine.rs - Guillotine splitting packer (simple alternative strategy).
//!
//! A straightforward guillotine bin packer used as one strategy in "best"
//! mode. Requests are placed largest-area first; every used free node is
//! split into two leftover strips (right and below). It is not as good as
//! the skyline packers in general but can help in some cases, and each
//! placement is O(free-list scan) with no skyline bookkeeping.

// Node is one free (or consumed) rectangle of the guillotine layout: x/y
// is its top-left corner, w/h its extent, and used marks it as consumed
// (kept in the list because the split step below still reads its fields).
#[derive(Clone, Debug)]
struct Node {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    used: bool,
}

/// Rect is one guillotine placement outcome, index-aligned with the
/// request list passed to [`pack_guillotine`].
///
/// When packed is false the request did not fit and the other fields are
/// meaningless (all zero). Otherwise the content occupies
/// `[x, x+w) × [y, y+h)`; the padding passed to [`pack_guillotine`] is not
/// part of the rectangle, it only shifts x/y outward.
#[derive(Clone, Debug)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub packed: bool,
}

/// pack_guillotine places every (w, h) request into a width × height bin,
/// largest-area first, and returns one [`Rect`] per request,
/// index-aligned with requests.
///
/// A zero-extent request is "packed" at the origin without consuming
/// space. A bin with non-positive dimensions rejects everything. Results
/// report the content origin (padded slot origin + padding).
pub fn pack_guillotine(requests: &[(i32, i32)], width: i32, height: i32, padding: i32) -> Vec<Rect> {
    let mut results = vec![
        Rect {
            x: 0,
            y: 0,
            w: 0,
            h: 0,
            packed: false,
        };
        requests.len()
    ];
    if width <= 0 || height <= 0 {
        return results;
    }

    let width_ = width;
    let height_ = height;
    let mut free: Vec<Node> = vec![Node {
        x: 0,
        y: 0,
        w: width_,
        h: height_,
        used: false,
    }];

    // try largest first by area
    let mut order: Vec<usize> = (0..requests.len()).collect();
    order.sort_by(|&a, &b| {
        (requests[b].0 * requests[b].1).cmp(&(requests[a].0 * requests[a].1))
    });

    for &idx in &order {
        let (rw, rh) = requests[idx];
        // Padded request size. Non-positive padded extents (zero-extent
        // requests with no padding, or negative request sizes) need no
        // space and are marked packed at the origin.
        let w = rw + padding * 2;
        let h = rh + padding * 2;
        if w <= 0 || h <= 0 {
            results[idx] = Rect { x: 0, y: 0, w: rw, h: rh, packed: true };
            continue;
        }
        // find best fit node
        let mut best: Option<(usize, i32)> = None; // (index, waste)
        for (i, n) in free.iter().enumerate() {
            if !n.used && n.w >= w && n.h >= h {
                let waste = (n.w - w) * (n.h - h);
                match best {
                    None => best = Some((i, waste)),
                    Some((_, bw)) if waste < bw => best = Some((i, waste)),
                    _ => {}
                }
            }
        }
        if let Some((ni, _)) = best {
            let fx = free[ni].x;
            let fy = free[ni].y;
            free[ni].used = true;
            results[idx] = Rect { x: fx + padding, y: fy + padding, w: rw, h: rh, packed: true };
            // Split the used node into two leftovers: a horizontal strip
            // to the right of the placed rect (the node's full height) and
            // a vertical strip below it. The vertical strip keeps the
            // placed rect's padded width — anything to its right inside
            // the node's bottom band is not carved out, so space there is
            // lost; a trade chosen for simplicity.
            let free_w = free[ni].w - w;
            let free_h = free[ni].h - h;
            if free_w > 0 {
                free.push(Node { x: fx + w, y: fy, w: free_w, h: free[ni].h, used: false });
            }
            if free_h > 0 {
                free.push(Node { x: fx, y: fy + h, w, h: free_h, used: false });
            }
        }
    }
    results
}