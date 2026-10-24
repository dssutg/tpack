//! genetic.rs - Genetic algorithm based packing for maximum packing density.
//!
//! Evolves permutation/order and simple placement choices to try to fit
//! more rectangles. Deterministic given a seed, for reproducibility.
//!
//! Each individual is an ordering of the requests; an order's fitness is
//! evaluated by running the skyline packer (both heuristics, without
//! pre-sorting) with that order and counting packed rectangles, with the
//! used area as a tie-break. Every generation keeps the elite pairs,
//! breeds children by order-crossover (an OX1-style splice from the first
//! parent completed from the second), and occasionally swap-mutates. The
//! arithmetic is all plain i32/usize with a small linear-congruential
//! generator, so the same seed always produces the same layout.

use crate::{Heuristic, Packer, Request, Result};

#[derive(Clone)]
struct Individual {
    order: Vec<usize>,
    fitness: i32,
}

// fitness_for_order evaluates one ordering: runs both skyline heuristics
// over the reordered requests and reports the better outcome plus the
// fitness used to rank individuals. Fitness is packed-count (highest
// priority) multiplied out of the way of the used area (tie-break), which
// is why it is scaled by 1_000_000.
fn fitness_for_order(order: &[usize], requests: &[Request], width: i32, height: i32, padding: i32) -> (Vec<Result>, bool, i32) {
    let mut ordered: Vec<Request> = vec![Request { w: 0, h: 0 }; order.len()];
    for (i, &orig) in order.iter().enumerate() {
        ordered[i] = requests[orig];
    }
    let mut packer = Packer::default();
    packer.init(width, height, ordered.len() as i32, Heuristic::Skyline_BF_sortHeight);
    let (res_bf, all_bf) = packer.pack(&ordered, padding, false);
    let mut packer2 = Packer::default();
    packer2.init(width, height, ordered.len() as i32, Heuristic::Skyline_default);
    let (res_bl, all_bl) = packer2.pack(&ordered, padding, false);
    let (res, all_packed) = if count_packed(&res_bf) >= count_packed(&res_bl) { (res_bf, all_bf) } else { (res_bl, all_bl) };
    let packed = count_packed(&res);
    let area = total_area(&res);
    (res, all_packed, packed * 1_000_000 + area)
}

fn count_packed(res: &[Result]) -> i32 {
    res.iter().filter(|r| r.was_packed).count() as i32
}

fn total_area(res: &[Result]) -> i32 {
    res.iter().filter(|r| r.was_packed).map(|r| r.w * r.h).sum()
}

// swap_mutate swaps two random positions of order in place. rng_state is a
// linear-congruential generator advanced twice per call: once for each
// swap index, so both positions move independently.
fn swap_mutate(order: &mut [usize], rng_state: &mut u64) {
    *rng_state = rng_state.wrapping_mul(1103515245).wrapping_add(12345);
    let i = (*rng_state as usize) % order.len();
    *rng_state = rng_state.wrapping_mul(1103515245).wrapping_add(12345);
    let j = (*rng_state as usize) % order.len();
    order.swap(i, j);
}

// crossover splices the two parents into one child order: a random slice
// of the first parent is copied, and the remaining slots are filled from
// the second parent in order, skipping values the child already has. This
// keeps the child a valid permutation when it terminates.
//
// rng_state advances once for the splice start and once more for the
// (start-relative) splice length.
fn crossover(a: &[usize], b: &[usize], rng_state: &mut u64) -> Vec<usize> {
    let n = a.len();
    if n < 2 {
        // An order shorter than two cannot be recombined; the child is
        // the first parent unchanged. (This also keeps the modulo below
        // away from a division by zero for single-element orders.)
        return a.to_vec();
    }
    let mut child = vec![usize::MAX; n];
    *rng_state = rng_state.wrapping_mul(1103515245).wrapping_add(12345);
    let start = (*rng_state as usize) % n;
    *rng_state = rng_state.wrapping_mul(1103515245).wrapping_add(12345);
    let end = start + 1 + ((*rng_state as usize) % (n.saturating_sub(1)));
    let end = end.min(n);
    child[start..end].copy_from_slice(&a[start..end]);
    let mut j = 0;
    for i in 0..n {
        if child[i] == usize::MAX {
            while j < n && child.contains(&b[j]) {
                j += 1;
            }
            if j < n {
                child[i] = b[j];
                j += 1;
            } else {
                // Defensive only, and unreachable in practice: the child
                // misses n - (end - start) values and b holds each of them
                // exactly once beyond the values already skipped (skipped
                // values are in the child for good), so j cannot exhaust
                // before every slot is filled.
                child[i] = i;
            }
        }
    }
    child
}

/// pack_genetic packs with the genetic strategy under the built-in
/// defaults (seed 123456789, population 40, 60 generations). Most callers
/// pass their own tunables to [`pack_genetic_with_seed`] instead.
pub fn pack_genetic(requests: &[Request], width: i32, height: i32, padding: i32) -> (Vec<Result>, bool) {
    pack_genetic_with_seed(requests, width, height, padding, 123456789u64, 40, 60)
}

/// pack_genetic_with_seed packs the requests with the genetic strategy.
///
/// The evolved orders are evaluated with the skyline packer; the best
/// order found is re-packed with both skyline heuristics and the result
/// that packed more rectangles is returned. The returned placements are
/// index-aligned with requests regardless of the evolved ordering.
///
/// A seed of 0, a population of 0, or 0 generations falls back to the
/// defaults (see [`pack_genetic`]) instead of degenerating.
pub fn pack_genetic_with_seed(
    requests: &[Request],
    width: i32,
    height: i32,
    padding: i32,
    seed: u64,
    pop_size: usize,
    gens: usize,
) -> (Vec<Result>, bool) {
    let n = requests.len();
    if n == 0 {
        return (vec![], true);
    }
    let mut rng_state: u64 = if seed == 0 { 123456789u64 } else { seed };
    let pop_size = if pop_size == 0 { 40 } else { pop_size };
    let gens = if gens == 0 { 60 } else { gens };

    let mut pop: Vec<Individual> = Vec::with_capacity(pop_size);
    // Seed the population with two individuals before evolving: the
    // requests in their original order, and sorted by area (largest
    // first). Two unconditional seeds also guarantee the population holds
    // at least two individuals, which the generation loop below relies on.
    let base: Vec<usize> = (0..n).collect();
    let (_res, _all, fit) = fitness_for_order(&base, requests, width, height, padding);
    pop.push(Individual { order: base.clone(), fitness: fit });
    let mut by_area: Vec<usize> = (0..n).collect();
    by_area.sort_by(|&a, &b| (requests[b].w * requests[b].h).cmp(&(requests[a].w * requests[a].h)));
    let (_res2, _all2, fit2) = fitness_for_order(&by_area, requests, width, height, padding);
    pop.push(Individual { order: by_area, fitness: fit2 });
    // rest random-ish
    while pop.len() < pop_size {
        let mut order = base.clone();
        for _ in 0..((n / 4) + 1).max(1) {
            swap_mutate(&mut order, &mut rng_state);
        }
        let (_res, _all, fit) = fitness_for_order(&order, requests, width, height, padding);
        pop.push(Individual { order, fitness: fit });
    }

    // Generational loop: rank, keep the two elites, and breed the rest of
    // the population from tournament-selected parents.
    for _gen in 0..gens {
        pop.sort_by_key(|a| std::cmp::Reverse(a.fitness));
        let mut new_pop: Vec<Individual> = Vec::with_capacity(pop_size);
        new_pop.push(pop[0].clone());
        if pop.len() > 1 {
            new_pop.push(pop[1].clone());
        }
        while new_pop.len() < pop_size {
            // pop always holds at least two individuals (two seeds are
            // pushed unconditionally), so the half-population limit is at
            // least one and every index below is in range.
            let limit = pop.len() / 2;
            let idx1 = (rng_state as usize) % limit;
            let idx2 = (rng_state / 7) as usize % limit;
            rng_state = rng_state.wrapping_mul(1103515245).wrapping_add(12345);
            let parent1 = if pop[idx1].fitness >= pop[idx2].fitness {
                pop[idx1].clone()
            } else {
                pop[idx2].clone()
            };
            let idx3 = (rng_state as usize) % limit;
            rng_state = rng_state.wrapping_mul(1103515245).wrapping_add(12345);
            let idx4 = (rng_state / 13) as usize % limit;
            let parent2 = if pop[idx3].fitness >= pop[idx4].fitness {
                pop[idx3].clone()
            } else {
                pop[idx4].clone()
            };
            let mut child_order = crossover(&parent1.order, &parent2.order, &mut rng_state);
            if (rng_state as usize).is_multiple_of(3) {
                swap_mutate(&mut child_order, &mut rng_state);
            }
            let (_res, _all, fit) = fitness_for_order(&child_order, requests, width, height, padding);
            new_pop.push(Individual { order: child_order, fitness: fit });
        }
        pop = new_pop;
    }
    // Re-pack the best evolved order with both skyline heuristics and
    // return the outcome that packed more rectangles, mapped back onto
    // the original request indices.
    pop.sort_by_key(|a| std::cmp::Reverse(a.fitness));
    let best_order = pop[0].order.clone();
    let mut ordered: Vec<Request> = vec![Request { w: 0, h: 0 }; best_order.len()];
    for (i, &orig) in best_order.iter().enumerate() {
        ordered[i] = requests[orig];
    }
    let mut packer = Packer::default();
    packer.init(width, height, ordered.len() as i32, Heuristic::Skyline_BF_sortHeight);
    let (res_bf, all_bf) = packer.pack(&ordered, padding, false);
    let mut packer2 = Packer::default();
    packer2.init(width, height, ordered.len() as i32, Heuristic::Skyline_default);
    let (res_bl, all_bl) = packer2.pack(&ordered, padding, false);
    if count_packed(&res_bf) >= count_packed(&res_bl) {
        let mut final_res = vec![Result::default(); n];
        for (i, &orig) in best_order.iter().enumerate() {
            final_res[orig] = res_bf[i];
        }
        let all = all_bf && count_packed(&final_res) == n as i32;
        (final_res, all)
    } else {
        let mut final_res = vec![Result::default(); n];
        for (i, &orig) in best_order.iter().enumerate() {
            final_res[orig] = res_bl[i];
        }
        let all = all_bl && count_packed(&final_res) == n as i32;
        (final_res, all)
    }
}
