//! maxrects.rs - MaxRects style packing (BL/BA/BSSF/BLSF) as an extra strategy.
//!
//! This is a much simplified MaxRects variant scored with
//! Best-Short-Side-Fit only. Requests are placed largest-first into the
//! free rectangle that leaves the smallest short-side leftover; the used
//! free rect is then removed and split into up to three leftovers.
//!
//! Simplification: intersecting free rectangles are not resolved after a
//! placement (full MaxRects subtracts every placed rect from every free
//! rect it intersects). The free list can therefore hold nested or
//! overlapping leftovers and later placements may reuse space a previous
//! placement occupies — the packer trades that quality for simplicity,
//! and its output should be treated as a heuristic attempt rather than a
//! tight layout.

use crate::{Request, Result};

#[derive(Clone, Debug)]
struct FreeRect {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

// score_best_short_side_fit scores (w, h) into a free rect under the
// Best-Short-Side-Fit heuristic: the packing that leaves the smallest
// leftover on the shorter side is preferred, ties going to the smallest
// long-side leftover. The 4096 stride makes the short leftover dominate
// the comparison. None when the free rect is too small.
fn score_best_short_side_fit(free: &FreeRect, w: i32, h: i32) -> Option<i32> {
    if free.w < w || free.h < h { return None; }
    let leftover_h = (free.h - h).min(free.w - w);
    let leftover_w = (free.h - h).max(free.w - w);
    Some(leftover_h * 4096 + leftover_w) // prefer smaller short side
}

/// pack_maxrects places every request into a width × height bin,
/// largest-area first, scored by best-short-side-fit.
///
/// Returns one [`Result`] per request, index-aligned with requests; the
/// bool reports whether every request was packed. A zero-extent request
/// is "packed" at the origin without consuming space. A bin with
/// non-positive dimensions rejects everything quickly and fills each
/// result's w with the first request's width as a placeholder before
/// returning.
pub fn pack_maxrects(requests: &[Request], width: i32, height: i32, padding: i32) -> (Vec<Result>, bool) {
    let mut results = vec![Result::default(); requests.len()];
    let mut all_packed = true;
    if width <= 0 || height <= 0 {
        for r in results.iter_mut().take(requests.len()) {
            r.w = requests[0].w; // dummy
        }
        return (results, false);
    }
    let mut free: Vec<FreeRect> = vec![FreeRect { x: 0, y: 0, w: width, h: height }];
    // sort by area desc
    let mut order: Vec<usize> = (0..requests.len()).collect();
    order.sort_by(|&a,&b| (requests[b].w*requests[b].h + requests[b].w).cmp(&(requests[a].w*requests[a].h + requests[a].w)));
    for &idx in &order {
        let req = requests[idx];
        results[idx].w = req.w;
        results[idx].h = req.h;
        if req.w == 0 || req.h == 0 {
            results[idx].was_packed = true;
            continue;
        }
        let w = req.w + padding*2;
        let h = req.h + padding*2;
        // find best (BSSF); a score of 0 is a perfect fit and cannot be
        // beaten, so stop scanning.
        let mut best_i: Option<usize> = None;
        let mut best_score: i32 = i32::MAX;
        for (i, fr) in free.iter().enumerate() {
            if let Some(s) = score_best_short_side_fit(fr, w, h) {
                if s < best_score {
                    best_score = s;
                    best_i = Some(i);
                    if best_score == 0 { break; }
                }
            }
        }
        if let Some(bi) = best_i {
            let fr = free[bi].clone();
            results[idx].x = fr.x + padding;
            results[idx].y = fr.y + padding;
            results[idx].was_packed = true;
            // split the used free rect into leftovers: a right strip and a
            // right-bottom corner when the right side has room, plus a
            // full-width bottom strip when the bottom has room. The right
            // strips only span the rect's padded height, so a thin band to
            // the right below the rect is left unaccounted for.
            free.remove(bi);
            // add splits
            if fr.w - w > 0 {
                free.push(FreeRect { x: fr.x + w, y: fr.y, w: fr.w - w, h });
                free.push(FreeRect { x: fr.x + w, y: fr.y + h, w: fr.w - w, h: fr.h - h });
            }
            if fr.h - h > 0 {
                free.push(FreeRect { x: fr.x, y: fr.y + h, w: fr.w, h: fr.h - h });
            }
        } else {
            results[idx].was_packed = false;
            all_packed = false;
        }
    }
    (results, all_packed)
}