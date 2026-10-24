// skyline_alt.rs - Alternative skyline orderings/tries

use crate::{Heuristic, Packer, Request, Result};

pub fn pack_skyline_multi(requests: &[Request], width: i32, height: i32, padding: i32) -> (Vec<Result>, bool) {
    let mut best_res: Option<Vec<Result>> = None;
    let mut best_count = -1;
    let mut best_all = false;
    // try different sort orders
    let sorts = [true, false];
    let heuristics = [Heuristic::Skyline_default, Heuristic::Skyline_BF_sortHeight];
    for &sort in &sorts {
        for &h in &heuristics {
            let mut packer = Packer::default();
            packer.init(width, height, requests.len() as i32, h);
            let (res, all) = packer.pack(requests, padding, sort);
            let c = res.iter().filter(|r| r.was_packed).count() as i32;
            if c > best_count {
                best_count = c;
                best_all = all;
                best_res = Some(res);
            }
        }
    }
    (best_res.unwrap_or_else(|| vec![Result::default(); requests.len()]), best_all && best_count == requests.len() as i32)
}
