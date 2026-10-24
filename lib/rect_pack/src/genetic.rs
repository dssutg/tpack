// genetic.rs - Genetic algorithm based packing for maximum packing density.
//
// Evolves permutation/order and simple placement choices to try to fit more
// rectangles. Deterministic given seed for reproducibility.

use crate::{Heuristic, Packer, Request, Result};

#[derive(Clone)]
struct Individual {
    order: Vec<usize>,
    fitness: i32,
}

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

fn swap_mutate(order: &mut [usize], rng_state: &mut u64) {
    *rng_state = rng_state.wrapping_mul(1103515245).wrapping_add(12345);
    let i = (*rng_state as usize) % order.len();
    *rng_state = rng_state.wrapping_mul(1103515245).wrapping_add(12345);
    let j = (*rng_state as usize) % order.len();
    order.swap(i, j);
}

fn crossover(a: &[usize], b: &[usize], rng_state: &mut u64) -> Vec<usize> {
    let n = a.len();
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
                child[i] = i;
            }
        }
    }
    child
}

pub fn pack_genetic(requests: &[Request], width: i32, height: i32, padding: i32) -> (Vec<Result>, bool) {
    pack_genetic_with_seed(requests, width, height, padding, 123456789u64, 40, 60)
}

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
    // individual 0: original order
    let base: Vec<usize> = (0..n).collect();
    let (_res, _all, fit) = fitness_for_order(&base, requests, width, height, padding);
    pop.push(Individual { order: base.clone(), fitness: fit });
    // individual 1: by area desc
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

    for _gen in 0..gens {
        pop.sort_by_key(|a| std::cmp::Reverse(a.fitness));
        let mut new_pop: Vec<Individual> = Vec::with_capacity(pop_size);
        new_pop.push(pop[0].clone());
        if pop.len() > 1 {
            new_pop.push(pop[1].clone());
        }
        while new_pop.len() < pop_size {
            let limit = pop.len() / 2;
            let idx1 = if limit == 0 { 0 } else { (rng_state as usize) % limit };
            let idx2 = (rng_state / 7) as usize % if limit == 0 { 1 } else { limit };
            rng_state = rng_state.wrapping_mul(1103515245).wrapping_add(12345);
            let parent1 = if pop[idx1].fitness >= pop[idx2].fitness {
                pop[idx1].clone()
            } else {
                pop[idx2].clone()
            };
            let idx3 = if limit == 0 { 0 } else { (rng_state as usize) % limit };
            rng_state = rng_state.wrapping_mul(1103515245).wrapping_add(12345);
            let idx4 = (rng_state / 13) as usize % if limit == 0 { 1 } else { limit };
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
