// genetic_test.rs - behavioral tests for the genetic packing strategy.
//
// These stay black-box: they assert what the strategy guarantees
// (determinism, index alignment, fit or not, in-bounds placements, the
// degenerate-input fallbacks), never how individuals or the RNG are
// represented internally.

use rect_pack::{Request, Result};
use rect_pack::genetic::{pack_genetic, pack_genetic_with_seed};

fn count_packed(res: &[Result]) -> i32 {
    res.iter().filter(|r| r.was_packed).count() as i32
}

// A bin and request set where a dense layout is possible; padding 1 keeps
// sprites separated exactly like the packer's typical use. On this layout
// the GA's final re-pack picks Bottom-Left over Best-Fit, and every seed
// below packs all four rectangles in it.
const DENSE_SET: [Request; 4] = [
    Request { w: 7, h: 2 },
    Request { w: 7, h: 4 },
    Request { w: 11, h: 10 },
    Request { w: 9, h: 2 },
];

// Empty input packs trivially to an empty, fully-successful result.
#[test]
fn empty_requests() {
    let (res, all) = pack_genetic_with_seed(&[], 64, 64, 1, 1, 40, 60);
    assert!(res.is_empty());
    assert!(all);
    let (res, all) = pack_genetic(&[], 64, 64, 1);
    assert!(res.is_empty());
    assert!(all);
}

// A single request must not panic: the order recombination must handle
// one-element orders (this was a division-by-zero crash before the n < 2
// guard existed).
#[test]
fn single_request_does_not_panic() {
    let requests = [Request { w: 13, h: 5 }];
    let (res, all) = pack_genetic(&requests, 64, 64, 1);
    assert!(all);
    assert_eq!(count_packed(&res), 1);
    assert!(res[0].was_packed);
    assert_eq!((res[0].x, res[0].y), (1, 1)); // content origin = padding
    assert_eq!((res[0].w, res[0].h), (13, 5));

    // Two elements exercise crossover on the smallest recombining size.
    let requests = [Request { w: 13, h: 5 }, Request { w: 4, h: 4 }];
    let (res, all) = pack_genetic(&requests, 64, 64, 1);
    assert!(all);
    assert_eq!(count_packed(&res), 2);
}

// Degenerate parameters (0) fall back to the defaults instead of
// misbehaving; 0-gens and 0-pop must equal the built-in defaults' result.
#[test]
fn zero_parameters_fall_back_to_defaults() {
    let requests = &DENSE_SET[..];
    let (a, all_a) = pack_genetic_with_seed(requests, 25, 18, 1, 0, 0, 0);
    let (b, all_b) = pack_genetic_with_seed(requests, 25, 18, 1, 123456789, 40, 60);
    assert_eq!(a, b);
    assert_eq!(all_a, all_b);

    // An explicit seed of 0 behaves like the default seed as well.
    let (c, _) = pack_genetic_with_seed(requests, 25, 18, 1, 0, 40, 60);
    assert_eq!(c, b);

    // A tiny population is still valid: the two seeded individuals are
    // processed and the result stays index-aligned.
    let (d, _) = pack_genetic_with_seed(requests, 25, 18, 1, 7, 1, 5);
    assert_eq!(d.len(), requests.len());
    for (i, r) in d.iter().enumerate() {
        assert_eq!(r.w, requests[i].w);
        assert_eq!(r.h, requests[i].h);
    }
}

// Same parameters must pack identically every time.
#[test]
fn deterministic_given_seed() {
    let requests = &DENSE_SET[..];
    let (a, all_a) = pack_genetic_with_seed(requests, 25, 18, 1, 7, 8, 5);
    let (b, all_b) = pack_genetic_with_seed(requests, 25, 18, 1, 7, 8, 5);
    assert_eq!(a, b);
    assert_eq!(all_a, all_b);
}

// Whatever packing succeeds must be inside the bin, aligned with the
// requests, and non-overlapping. (The GA's skylines are the same
// placements as the skyline packer's, so overlap-freedom holds like the
// skyline tests.)
#[test]
fn dense_set_packing_is_well_formed() {
    for seed in [1u64, 2, 3, 7, 11, 123456789] {
        let (res, _) = pack_genetic_with_seed(&DENSE_SET, 25, 18, 1, seed, 4, 3);
        assert_eq!(res.len(), 4);
        for (i, r) in res.iter().enumerate() {
            assert_eq!(r.w, DENSE_SET[i].w);
            assert_eq!(r.h, DENSE_SET[i].h);
            if r.was_packed {
                assert!(r.x >= 0 && r.y >= 0);
                assert!(r.x + r.w <= 25 && r.y + r.h <= 18);
            }
        }
        // padded slots must not overlap
        for (a, ra) in res.iter().enumerate() {
            if !ra.was_packed {
                continue;
            }
            for (b, rb) in res.iter().enumerate().skip(a + 1) {
                if !rb.was_packed {
                    continue;
                }
                let overlap = ra.x - 1 < rb.x + rb.w
                    && rb.x - 1 < ra.x + ra.w
                    && ra.y - 1 < rb.y + rb.h
                    && rb.y - 1 < ra.y + ra.h;
                assert!(!overlap, "seed {seed}: rects {a} and {b} overlap");
            }
        }
    }
}

// The GA's final selection must take the better of the two heuristic
// re-packs: on this exact layout Bottom-Left packs strictly more of the
// best order than Best-Fit, so the BL result wins and everything packs.
#[test]
fn dense_bin_packs_everything() {
    let (res, all) = pack_genetic_with_seed(&DENSE_SET, 25, 18, 1, 7, 4, 3);
    assert!(all, "every request packs in the 25x18 bin");
    assert_eq!(count_packed(&res), 4);
    // same call is stable
    let (res2, all2) = pack_genetic_with_seed(&DENSE_SET, 25, 18, 1, 7, 4, 3);
    assert_eq!(res, res2);
    assert_eq!(all, all2);
}

// A set that cannot fit at all must return unpacked results for every
// request rather than panicking or claiming success.
#[test]
fn impossible_layout_reports_unpacked() {
    let requests = [Request { w: 100, h: 100 }, Request { w: 101, h: 101 }];
    let (res, all) = pack_genetic(&requests, 64, 64, 1);
    assert!(!all);
    assert!(!res[0].was_packed);
    assert!(!res[1].was_packed);
}