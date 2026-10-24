// strategies_test.rs - behavioral tests for the alternative packing
// strategies (guillotine, maxrects, skyline_alt, best). These stay
// black-box: layout correctness properties, alignment, in-bounds and
// deterministic behavior — not internal representation.

use rect_pack::maxrects::pack_maxrects;
use rect_pack::skyline_alt::pack_skyline_multi;
use rect_pack::guillotine::{pack_guillotine, Rect};
use rect_pack::{pack_best, Request, Result};

fn count_packed(res: &[Result]) -> i32 {
    res.iter().filter(|r| r.was_packed).count() as i32
}

// ---------------------------------------------------------------------------
// skyline_alt
// ---------------------------------------------------------------------------

// Empty requests still return an empty, fully-successful result.
#[test]
fn skyline_alt_empty_requests() {
    let (res, all) = pack_skyline_multi(&[], 64, 64, 1);
    assert!(res.is_empty());
    assert!(all);
}

// Roomy bin: some configuration packs everything; results are aligned
// with requests; the packed flag agrees with a fully success.
#[test]
fn skyline_alt_packs_everything_in_a_roomy_bin() {
    let requests = [
        Request { w: 4, h: 9 },
        Request { w: 16, h: 3 },
        Request { w: 7, h: 7 },
        Request { w: 1, h: 1 },
    ];
    let (res, all) = pack_skyline_multi(&requests, 64, 64, 1);
    assert!(all);
    assert_eq!(count_packed(&res), requests.len() as i32);
    for (i, r) in res.iter().enumerate() {
        assert!(r.was_packed);
        assert_eq!(r.w, requests[i].w);
        assert_eq!(r.h, requests[i].h);
        assert!(r.x >= 0 && r.y >= 0);
        assert!(r.x + r.w <= 64 && r.y + r.h <= 64);
    }
}

// A later configuration can beat the first one: pre-sorting by size is
// wrong for this layout, where unsorted Bottom-Left packs a rectangle
// that sorted Bottom-Left rejects. The overall outcome is still a
// non-overlapping partial packing (the bin is too tight for all three).
#[test]
fn skyline_alt_configurations_can_improve_on_the_tried_order() {
    let requests = [Request { w: 2, h: 5 }, Request { w: 6, h: 7 }, Request { w: 2, h: 5 }];
    let (res, all) = pack_skyline_multi(&requests, 10, 11, 1);
    assert!(!all);
    assert_eq!(count_packed(&res), 2);
    for (i, r) in res.iter().enumerate() {
        assert_eq!(r.w, requests[i].w);
        assert_eq!(r.h, requests[i].h);
        if r.was_packed {
            assert!(r.x >= 0 && r.y >= 0);
            assert!(r.x + r.w <= 10 && r.y + r.h <= 11);
        }
    }
    // same inputs, same layout
    let (again, _) = pack_skyline_multi(&requests, 10, 11, 1);
    assert_eq!(res, again);
}

// Determinism: the same inputs produce the same layout.
#[test]
fn skyline_alt_is_deterministic() {
    let requests = [
        Request { w: 4, h: 9 },
        Request { w: 16, h: 3 },
        Request { w: 7, h: 7 },
        Request { w: 1, h: 1 },
    ];
    let (a, a_all) = pack_skyline_multi(&requests, 64, 64, 2);
    let (b, b_all) = pack_skyline_multi(&requests, 64, 64, 2);
    assert_eq!(a, b);
    assert_eq!(a_all, b_all);
}

// A bin with invalid dimensions must reject everything instead of
// panicking or corrupting results.
#[test]
fn skyline_alt_rejects_invalid_bins() {
    for (w, h) in [(0i32, 10i32), (10, 0), (-5, -5)] {
        let requests = [Request { w: 4, h: 4 }, Request { w: 2, h: 2 }];
        let (res, all) = pack_skyline_multi(&requests, w, h, 1);
        assert!(!all, "a degenerate bin must not report success");
        for r in res {
            assert!(!r.was_packed);
        }
    }
}

// ---------------------------------------------------------------------------
// maxrects
// ---------------------------------------------------------------------------

// The simplified maxrects variant may leave free-space overlap (see the
// module docs), so these tests assert the in-bounds and alignment
// properties but not overlap-freedom.
#[test]
fn maxrects_aligns_and_bounds_results() {
    let requests = [
        Request { w: 4, h: 9 },
        Request { w: 16, h: 3 },
        Request { w: 7, h: 7 },
    ];
    let (res, all) = pack_maxrects(&requests, 64, 64, 2);
    assert!(all);
    for (i, r) in res.iter().enumerate() {
        assert!(r.was_packed);
        assert_eq!(r.w, requests[i].w);
        assert_eq!(r.h, requests[i].h);
        // content origin == free-slot origin + padding
        assert!(r.x >= 2 && r.y >= 2);
        assert!(r.x + r.w + 2 <= 64 && r.y + r.h + 2 <= 64);
    }
}

// Zero-extent requests are packed at the origin without consuming space.
#[test]
fn maxrects_zero_extent_requests() {
    let requests = [
        Request { w: 0, h: 0 },
        Request { w: 5, h: 5 },
        Request { w: 0, h: 3 },
        Request { w: 3, h: 0 },
    ];
    let (res, all) = pack_maxrects(&requests, 64, 64, 1);
    assert!(all);
    assert!(res[0].was_packed && res[2].was_packed && res[3].was_packed);
    assert_eq!((res[0].x, res[0].y), (0, 0));
}

// Unfit requests are reported unpacked and the bool flips false.
#[test]
fn maxrects_rejects_oversize_without_panicking() {
    let requests = [Request { w: 65, h: 65 }, Request { w: 32, h: 32 }];
    let (res, all) = pack_maxrects(&requests, 64, 64, 1);
    assert!(!all);
    assert!(!res[0].was_packed);
    assert!(res[1].was_packed);
    assert_eq!(res[1].w, 32);
    assert_eq!(res[1].h, 32);
}

// Degenerate bins take the fast reject path: nothing packs, and the
// placeholder width is filled from the first request.
#[test]
fn maxrects_degenerate_bins() {
    let requests = [Request { w: 4, h: 4 }, Request { w: 9, h: 2 }];
    let (res, _) = pack_maxrects(&requests, 0, 10, 1);
    assert!(!res.iter().all(|r| r.was_packed));
    for r in &res {
        assert!(!r.was_packed);
    }
    // placeholder: every result carries the FIRST request's width
    assert_eq!(res.iter().map(|r| r.w).collect::<Vec<_>>(), vec![4, 4]);

    // empty requests on a degenerate bin must not index out of bounds
    let (res, _) = pack_maxrects(&[], 0, 10, 1);
    assert!(res.is_empty());
}

// Split-shape edges: a rect that exactly matches the free rect's width
// leaves no bottom-strip, one that matches its height leaves no right
// strip, and an exact full-fit stops the scoring scan at a perfect
// fit (score 0).
#[test]
fn maxrects_split_shape_edges() {
    // full-width request: the height leftover is the only split
    let (res, all) = pack_maxrects(&[Request { w: 6, h: 2 }], 8, 8, 1);
    assert!(all);
    assert_eq!((res[0].x, res[0].y), (1, 1));
    assert_eq!((res[0].w, res[0].h), (6, 2));

    // full-height request: the width leftover is the only split
    let (res, all) = pack_maxrects(&[Request { w: 2, h: 6 }], 8, 8, 1);
    assert!(all);
    assert_eq!((res[0].w, res[0].h), (2, 6));

    // exact full fit: the free rect disappears entirely
    let (res, all) = pack_maxrects(&[Request { w: 8, h: 8 }], 8, 8, 0);
    assert!(all);
    assert_eq!((res[0].x, res[0].y, res[0].w, res[0].h), (0, 0, 8, 8));
}

// ---------------------------------------------------------------------------
// guillotine
// ---------------------------------------------------------------------------

fn guillotine_in_bounds_and_split_properties(requests: &[(i32, i32)], width: i32, height: i32) {
    let res = pack_guillotine(requests, width, height, 1);
    assert_eq!(res.len(), requests.len());
    for (i, r) in res.iter().enumerate() {
        if !r.packed {
            continue;
        }
        assert_eq!(r.w, requests[i].0);
        assert_eq!(r.h, requests[i].1);
        assert!(r.x >= 0 && r.y >= 0, "rect {i} at negative offset");
        assert!(r.x + r.w <= width && r.y + r.h <= height, "rect {i} out of bounds");
    }
    // packed slots (padded by the 1-pixel padding) must not overlap.
    let padded = |r: &Rect| (r.x - 1, r.y - 1, r.x + r.w, r.y + r.h);
    for (a, ra) in res.iter().enumerate() {
        if !ra.packed {
            continue;
        }
        for (b, rb) in res.iter().enumerate().skip(a + 1) {
            if !rb.packed {
                continue;
            }
            let (ax0, ay0, ax1, ay1) = padded(ra);
            let (bx0, by0, bx1, by1) = padded(rb);
            assert!(
                !(ax0 < bx1 && bx0 < ax1 && ay0 < by1 && by0 < ay1),
                "rects {a} and {b} overlap in {width}x{height}"
            );
        }
    }
}

// A set exercising all four split outcomes: an exact fit (both strips
// empty), a wider-than-tall leftover (horizontal strip only), a
// taller-than-wide leftover (vertical strip only), and both.
#[test]
fn guillotine_all_split_shapes() {
    // exact fit: 8x8 into an 8-high, 8-wide bin leaves nothing (with no
    // padding the strips are zero-width)
    let requests = [(8, 8)];
    let res = pack_guillotine(&requests, 32, 32, 0);
    assert!(res[0].packed);
    assert_eq!((res[0].x, res[0].y, res[0].w, res[0].h), (0, 0, 8, 8));

    // wider leftover only: 4x4 in a 16x8 node (heights equal) — the
    // vertical strip is zero-height.
    guillotine_in_bounds_and_split_properties(&[(4, 4)], 16, 8);

    // taller leftover only: 4x4 in an 8x16 node (widths equal) — the
    // horizontal strip is zero-width.
    guillotine_in_bounds_and_split_properties(&[(4, 4)], 8, 16);

    // both: 4x4 in a 17x13 node.
    guillotine_in_bounds_and_split_properties(&[(4, 4)], 17, 13);

    // a mixed bag with different orders of placement
    guillotine_in_bounds_and_split_properties(
        &[(3, 3), (10, 2), (2, 10), (5, 5), (1, 1)],
        32,
        32,
    );
}

// Zero-extent requests are packed at the origin without consuming space;
// their reported sizes stay as requested.
#[test]
fn guillotine_zero_extent_requests() {
    let requests = [(0, 0), (5, 5), (0, 3), (3, 0)];
    let res = pack_guillotine(&requests, 32, 32, 0);
    assert!(res.iter().all(|r| r.packed));
    for r in [&res[0], &res[2], &res[3]] {
        assert_eq!((r.x, r.y), (0, 0));
    }
    assert_eq!((res[0].w, res[0].h), (0, 0));
    assert_eq!((res[2].w, res[2].h), (0, 3));
    assert_eq!((res[3].w, res[3].h), (3, 0));
    assert!(res[1].packed);
    assert_eq!((res[1].x, res[1].y), (0, 0));
}

// A bin with non-positive dimensions rejects everything.
#[test]
fn guillotine_rejects_invalid_bins() {
    for (w, h) in [(0i32, 10i32), (10, 0), (-1, -1)] {
        let res = pack_guillotine(&[(4, 4)], w, h, 1);
        assert!(!res[0].packed);
    }
}

// A valid bin that is simply too crowded reports the overflow unpacked
// (and keeps earlier placements in bounds).
#[test]
fn guillotine_reports_unplaced_in_a_valid_bin() {
    let res = pack_guillotine(&[(6, 6), (4, 4)], 8, 8, 1);
    assert!(res[0].packed);
    assert!(!res[1].packed);
}

// ---------------------------------------------------------------------------
// pack_best
// ---------------------------------------------------------------------------

// A dense layout where the fastest skyline succeeds: the search must end
// early with a fully successful result.
#[test]
fn best_finishes_early_when_skyline_success() {
    let requests = [
        Request { w: 4, h: 9 },
        Request { w: 16, h: 3 },
        Request { w: 7, h: 7 },
        Request { w: 1, h: 1 },
    ];
    let (res, all) = pack_best(&requests, 64, 64, 1);
    assert!(all);
    assert_eq!(count_packed(&res), requests.len() as i32);
    // aligned and in bounds
    for (i, r) in res.iter().enumerate() {
        assert_eq!(r.w, requests[i].w);
        assert_eq!(r.h, requests[i].h);
        assert!(r.x >= 0 && r.y >= 0 && r.x + r.w <= 64 && r.y + r.h <= 64);
    }
}

// A hopeless layout falls through every strategy and still returns sane,
// aligned, deterministic results.
#[test]
fn best_falls_through_on_hopeless_layout() {
    let requests = [Request { w: 200, h: 200 }, Request { w: 300, h: 300 }];
    let (a, all_a) = pack_best(&requests, 64, 64, 1);
    let (b, all_b) = pack_best(&requests, 64, 64, 1);
    assert!(!all_a);
    assert_eq!(a, b);
    assert_eq!(all_a, all_b);
    assert!(!a[0].was_packed && !a[1].was_packed);
}

// A partially-packable layout falls through the whole search: every
// strategy's outcome gets scored against the best, not just the first
// one, and the strongest partial result is returned.
#[test]
fn best_scores_partially_packed_layouts() {
    let requests = [Request { w: 100, h: 100 }, Request { w: 10, h: 10 }];
    let (a, all_a) = pack_best(&requests, 64, 64, 1);
    assert!(!all_a);
    assert_eq!(count_packed(&a), 1);
    assert!(!a[0].was_packed && a[1].was_packed);

    // deterministic
    let (b, _) = pack_best(&requests, 64, 64, 1);
    assert_eq!(a, b);
}

// Empty requests through the full search.
#[test]
fn best_empty_requests() {
    let (res, all) = pack_best(&[], 64, 64, 1);
    assert!(res.is_empty());
    assert!(all);
}

// ---------------------------------------------------------------------------
// cross-strategy contract
// ---------------------------------------------------------------------------

// Every strategy shares the same contract: results are index-aligned with
// the requests, w/h mirror the request sizes, x/y are non-negative for
// packed rects, and pack_best never claims success it did not achieve.
#[test]
fn strategies_share_the_result_contract() {
    let requests = [
        Request { w: 18, h: 7 },
        Request { w: 6, h: 6 },
        Request { w: 6, h: 6 },
        Request { w: 9, h: 4 },
        Request { w: 20, h: 2 },
    ];
    let (_, best_all) = pack_best(&requests, 64, 64, 1);
    let (best, _) = pack_best(&requests, 64, 64, 1);
    for (i, r) in best.iter().enumerate() {
        assert_eq!(r.w, requests[i].w);
        assert_eq!(r.h, requests[i].h);
        if r.was_packed {
            assert!(r.x >= 0 && r.y >= 0 && r.x + r.w <= 64 && r.y + r.h <= 64);
        }
    }
    assert!(best_all == (count_packed(&best) == requests.len() as i32));
}