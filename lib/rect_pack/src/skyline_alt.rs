//! skyline_alt.rs - Alternative skyline orderings/tries.
//!
//! Runs the basic skyline packer under several configurations — sorted and
//! unsorted request order, each skyline heuristic — and keeps the outcome
//! that packs the most rectangles. It introduces no new algorithm; it is
//! the "try harder" fallback step of [`crate::PackingMode::Best`].

use crate::{Heuristic, Packer, Request, Result};

/// pack_skyline_multi packs the requests with the skyline packer once per
/// configuration (pre-sorted/unsorted × Bottom-Left/Best-Fit) and returns
/// the result that packed the most rectangles.
///
/// Configurations are tried in a fixed order and ties keep the first
/// configuration that reached the count, so the outcome is deterministic.
/// The returned placements are index-aligned with requests, and the bool
/// reports whether the winning configuration packed every request.
pub fn pack_skyline_multi(requests: &[Request], width: i32, height: i32, padding: i32) -> (Vec<Result>, bool) {
    // (pre-sort by size, skyline heuristic), in the fixed try order.
    let configurations = [
        (true, Heuristic::Skyline_default),
        (true, Heuristic::Skyline_BF_sortHeight),
        (false, Heuristic::Skyline_default),
        (false, Heuristic::Skyline_BF_sortHeight),
    ];

    // The first configuration seeds the best-so-far (it always runs, so
    // there is always a result); the remaining three may improve on it.
    let mut packer = Packer::default();
    packer.init(width, height, requests.len() as i32, configurations[0].1);
    let (mut best_res, mut best_all) = packer.pack(requests, padding, configurations[0].0);
    let mut best_count = best_res.iter().filter(|r| r.was_packed).count() as i32;

    for &(sort, heuristic) in &configurations[1..] {
        let mut packer = Packer::default();
        packer.init(width, height, requests.len() as i32, heuristic);
        let (res, all) = packer.pack(requests, padding, sort);
        let count = res.iter().filter(|r| r.was_packed).count() as i32;
        if count > best_count {
            best_count = count;
            best_all = all;
            best_res = res;
        }
    }

    (best_res, best_all && best_count == requests.len() as i32)
}