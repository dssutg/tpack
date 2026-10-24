// maxrects.rs - MaxRects style packing (BL/BA/BSSF/BLSF) as an extra strategy.

use crate::{Request, Result};

#[derive(Clone, Debug)]
struct FreeRect {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

fn score_best_short_side_fit(free: &FreeRect, w: i32, h: i32) -> Option<i32> {
    if free.w < w || free.h < h { return None; }
    let leftover_h = (free.h - h).min(free.w - w);
    let leftover_w = (free.h - h).max(free.w - w);
    Some(leftover_h * 4096 + leftover_w) // prefer smaller short side
}

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
        // find best (BSSF)
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
            // split
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
