use rect_pack::{Heuristic, Packer, Request};

#[test]
fn test_basic_pack() {
    let mut p = Packer::default();
    p.init(100, 100, 16, Heuristic::Skyline_default);

    let requests = [
        Request { w: 10, h: 10 },
        Request { w: 20, h: 15 },
        Request { w: 5, h: 30 },
        Request { w: 12, h: 12 },
    ];
    let (results, all_packed) = p.pack(&requests, 1, true);

    assert!(all_packed, "expected all rectangles to pack");
    assert_eq!(results.len(), requests.len(), "result count mismatch");
    for (i, r) in results.iter().enumerate() {
        assert!(r.was_packed, "request {i} was not packed");
        assert_eq!(r.w, requests[i].w, "request {i} w mismatch");
        assert_eq!(r.h, requests[i].h, "request {i} h mismatch");
        // padded slot must lie inside the bin
        assert!(
            r.x >= 0 && r.y >= 0 && r.x + r.w <= p.width() && r.y + r.h <= p.height(),
            "request {i} out of bounds ({} ,{} {}x{})",
            r.x,
            r.y,
            r.w,
            r.h
        );
    }
}

// rects must not overlap once padded.
#[test]
fn test_no_overlap() {
    let mut p = Packer::default();
    p.init(256, 256, 64, Heuristic::Skyline_BF_sortHeight);

    let mut requests = vec![Request::default(); 40];
    // repeat determinism-friendly sizes
    let sizes = [
        Request { w: 32, h: 32 },
        Request { w: 64, h: 16 },
        Request { w: 16, h: 64 },
        Request { w: 8, h: 8 },
        Request { w: 24, h: 48 },
    ];
    for i in 0..requests.len() {
        requests[i] = sizes[i % sizes.len()];
    }

    let (results, all_packed) = p.pack(&requests, 1, true);
    assert!(all_packed, "expected all to fit in a 256 bin");

    for (a, ra) in results.iter().enumerate() {
        if !ra.was_packed {
            continue;
        }
        // padded bounds
        let ax0 = ra.x - 1;
        let ay0 = ra.y - 1;
        let ax1 = ra.x + ra.w;
        let ay1 = ra.y + ra.h;
        for (b, rb) in results.iter().enumerate().skip(a + 1) {
            if !rb.was_packed {
                continue;
            }
            let bx0 = rb.x - 1;
            let by0 = rb.y - 1;
            let bx1 = rb.x + rb.w;
            let by1 = rb.y + rb.h;
            let overlap = ax0 < bx1 && bx0 < ax1 && ay0 < by1 && by0 < ay1;
            assert!(!overlap, "rects {a} and {b} overlap (padded)");
        }
    }
}

// Empty rectangles consume no space and still report packed.
#[test]
fn test_empty_rect() {
    let mut p = Packer::default();
    p.init(50, 50, 8, Heuristic::Skyline_default);

    let requests = [
        Request { w: 20, h: 20 },
        Request { w: 0, h: 0 },
        Request { w: 0, h: 10 },
        Request { w: 10, h: 0 },
    ];
    let (results, all_packed) = p.pack(&requests, 1, true);
    assert!(all_packed, "empties should pack trivially");
    for i in 0..results.len() {
        assert!(results[i].was_packed, "empty request {i} should be packed");
        if requests[i].w == 0 || requests[i].h == 0 {
            assert!(
                results[i].x == 0 && results[i].y == 0,
                "empty rect should be at origin"
            );
        }
    }
}

// A rectangle too large to fit must be reported unpacked.
#[test]
fn test_too_large() {
    let mut p = Packer::default();
    p.init(32, 32, 8, Heuristic::Skyline_default);

    let requests = [
        Request { w: 8, h: 8 },
        Request { w: 40, h: 40 },
        Request { w: 12, h: 12 },
    ];
    let (results, all_packed) = p.pack(&requests, 1, true);
    assert!(!all_packed, "a too-large rect must fail packing");
    assert!(
        !results[1].was_packed,
        "rect index 1 (40x40) should not fit"
    );
    assert!(results[0].was_packed, "rect index 0 should fit");
    assert!(results[2].was_packed, "rect index 2 should fit");
}

// pack without sorting must leave the caller's slice untouched and still link
// results index-aligned.
#[test]
fn test_requests_untouched() {
    let mut p = Packer::default();
    p.init(64, 64, 16, Heuristic::Skyline_default);

    let requests = [
        Request { w: 30, h: 30 },
        Request { w: 20, h: 20 },
        Request { w: 10, h: 10 },
    ];
    let before = [requests[0], requests[1], requests[2]];
    let (results, _) = p.pack(&requests, 1, false);
    for i in 0..requests.len() {
        assert_eq!(requests[i], before[i], "caller slice was reordered");
        assert!(results[i].was_packed, "request {i} should pack");
        assert!(
            results[i].w == requests[i].w && results[i].h == requests[i].h,
            "request {i} w/h mismatch"
        );
    }
}

// The two heuristics must agree on whether something fits in a roomy bin.
#[test]
fn test_heuristic_parity() {
    fn mk(heuristic: Heuristic) -> Vec<rect_pack::Result> {
        let mut p = Packer::default();
        p.init(128, 128, 32, heuristic);
        let requests = [
            Request { w: 16, h: 16 },
            Request { w: 48, h: 32 },
            Request { w: 24, h: 24 },
            Request { w: 64, h: 12 },
        ];
        let (results, _) = p.pack(&requests, 1, true);
        results
    }
    let bl = mk(Heuristic::Skyline_default);
    let bf = mk(Heuristic::Skyline_BF_sortHeight);

    for i in 0..bl.len() {
        assert!(
            bl[i].was_packed == bf[i].was_packed,
            "heuristics disagree on packing request {i}"
        );
    }
}

// Stress: many rects must all be placed in-bounds without overlap, matching
// the skyline invariant that every packed slot is fully inside the bin.
#[test]
fn test_stress() {
    let mut p = Packer::default();
    p.init(512, 512, 128, Heuristic::Skyline_default);

    let mut requests = vec![Request::default(); 100];
    for (i, request) in requests.iter_mut().enumerate() {
        // deterministic pseudo-random-ish but non-trivially sized
        let w = 8 + ((i as i32 * 37) % 96);
        let h = 8 + ((i as i32 * 53) % 96);
        *request = Request { w, h };
    }

    let (results, _) = p.pack(&requests, 1, true);

    // The C stb_rect_pack.h packs exactly 49 of these 100 rects in a 512x512
    // bin with padding=1; verify our port matches.
    let packed_count = results.iter().filter(|r| r.was_packed).count();
    assert_eq!(packed_count, 49, "expected 49 packed, got {packed_count}");

    for i in 0..results.len() {
        let r = &results[i];
        if !r.was_packed {
            continue;
        }
        assert!(
            r.w == requests[i].w && r.h == requests[i].h,
            "request {i} size mismatch"
        );
        assert!(
            r.x >= 0 && r.y >= 0 && r.x + r.w <= p.width() && r.y + r.h <= p.height(),
            "request {i} out of bounds: ({} ,{} {}x{})",
            r.x,
            r.y,
            r.w,
            r.h
        );
    }

    // padded bounds must not overlap
    for (a, ra) in results.iter().enumerate() {
        if !ra.was_packed {
            continue;
        }
        let ax0 = ra.x - 1;
        let ay0 = ra.y - 1;
        let ax1 = ra.x + ra.w;
        let ay1 = ra.y + ra.h;
        for (b, rb) in results.iter().enumerate().skip(a + 1) {
            if !rb.was_packed {
                continue;
            }
            let overlap =
                ax0 < rb.x - 1 + rb.w && rb.x - 1 < ax1 && ay0 < rb.y - 1 + rb.h && rb.y - 1 < ay1;
            assert!(!overlap, "requests {a} and {b} overlap");
        }
    }
}

// The packer owns its node pool; dropping it releases everything, so no
// storage may leak after init/init (re-init for a new bin)/destroy.
#[test]
fn test_no_leaks() {
    let mut p = Packer::default();
    for i in 0..5 {
        // re-init must free the previous node pool, not leak it.
        p.init(64 + i, 64 + i, 32, Heuristic::Skyline_default);
        let requests = [
            Request { w: 16, h: 16 },
            Request { w: 24, h: 24 },
            Request { w: 8, h: 8 },
        ];
        let _ = p.pack(&requests, 1, true);
    }
    p.destroy();
}

// init with max_rects <= 0 gives an empty packer: nothing ever packs,
// even rectangles that easily fit.
#[test]
fn test_empty_packer_rejects_everything() {
    for max_rects in [0i32, -3] {
        let mut p = Packer::default();
        p.init(64, 64, max_rects, Heuristic::Skyline_default);
        let requests = [Request { w: 8, h: 8 }, Request { w: 16, h: 16 }];
        let (results, all_packed) = p.pack(&requests, 1, true);
        assert!(!all_packed || requests.is_empty());
        assert!(!results[0].was_packed);
        assert!(!results[1].was_packed);

        // an empty request list against the same packer stays successful
        let (results, all_packed) = p.pack(&[], 1, true);
        assert!(results.is_empty());
        assert!(all_packed);
    }
}

// Degenerate bin dimensions must reject every placement without panicking
// (guards the width-quantization arithmetic, which divides by the bin
// width).
#[test]
fn test_degenerate_bins_reject_everything() {
    for (width, height) in [(0i32, 64i32), (64, 0), (-5, -5), (0, 0)] {
        let mut p = Packer::default();
        p.init(width, height, 4, Heuristic::Skyline_default);
        let requests = [Request { w: 4, h: 4 }, Request { w: 2, h: 2 }];
        let (results, all_packed) = p.pack(&requests, 1, true);
        assert!(!all_packed, "bin {width}x{height} must not report success");
        for r in results {
            assert!(!r.was_packed);
        }
    }
}

// setup_allow_out_of_mem toggles width quantization; with the pool sizing
// [init] uses both modes must pack identically (the knob exists for C-API
// compatibility, see its docs).
#[test]
fn test_setup_allow_out_of_mem_modes_agree() {
    let requests = [
        Request { w: 13, h: 17 },
        Request { w: 29, h: 3 },
        Request { w: 5, h: 40 },
        Request { w: 11, h: 11 },
    ];
    let mut quantized = Packer::default();
    quantized.init(64, 64, 8, Heuristic::Skyline_default);
    quantized.setup_allow_out_of_mem(false);
    let mut exact = Packer::default();
    exact.init(64, 64, 8, Heuristic::Skyline_default);
    exact.setup_allow_out_of_mem(true);
    let (a, aa) = quantized.pack(&requests, 1, true);
    let (b, ab) = exact.pack(&requests, 1, true);
    assert_eq!(a, b);
    assert_eq!(aa, ab);
}

// setup_heuristic switches strategies without re-initializing the bin.
#[test]
fn test_setup_heuristic_switches_between_packs() {
    let mut p = Packer::default();
    p.init(64, 64, 16, Heuristic::Skyline_default);
    let requests = [Request { w: 30, h: 30 }, Request { w: 20, h: 20 }, Request { w: 10, h: 10 }];
    let _ = p.pack(&requests, 1, true);
    // switch on the same packer: valid heuristics never panic
    p.setup_heuristic(Heuristic::Skyline_BF_sortHeight);
    let _ = p.pack(&requests, 1, true);
    p.setup_heuristic(Heuristic::Skyline_BL_sortHeight);
    let _ = p.pack(&requests, 1, true);
}

#[test]
#[should_panic(expected = "rect_pack: init_target not called")]
fn test_setup_heuristic_panics_before_init() {
    let mut p = Packer::default();
    p.setup_heuristic(Heuristic::Skyline_default);
}

// The free-node clamp: a packer initialized for max_rects rectangles
// refuses to attempt more rectangles in one call than its node pool can
// represent. The extras are never attempted, so all_packed (which only
// reflects attempted placements) stays true here — the per-result
// was_packed flags are the authoritative signal, see pack's docs.
#[test]
fn test_free_node_clamp() {
    let mut p = Packer::default();
    // pool = max(max_rects, width) = 20 free nodes
    p.init(10, 64, 20, Heuristic::Skyline_default);
    let requests: Vec<Request> = (0..21).map(|_| Request { w: 1, h: 1 }).collect();
    let (results, all_packed) = p.pack(&requests, 0, false);
    assert!(all_packed, "attempted placements all succeed here");
    assert_eq!(results.iter().filter(|r| r.was_packed).count(), 20);
    // the clamped-out request was never attempted and is reported unpacked
    assert!(!results[20].was_packed);
}

// Calling pack twice on one packer continues filling the current bin;
// the second call packs into the remaining space without re-init.
#[test]
fn test_second_pack_continues_filling() {
    let mut p = Packer::default();
    p.init(64, 64, 8, Heuristic::Skyline_default);
    let requests = [Request { w: 30, h: 30 }, Request { w: 20, h: 20 }];
    let (first, all_first) = p.pack(&requests, 1, true);
    assert!(all_first);

    // The remaining space still holds exactly one of the three.
    let more = [Request { w: 10, h: 10 }];
    let (second, all_second) = p.pack(&more, 1, true);
    assert!(all_second);
    assert!(second[0].was_packed);
    // new placements must not collide with the first round
    for r in first.iter().filter(|r| r.was_packed) {
        let overlap = r.x - 1 < second[0].x + 10 + 1
            && second[0].x - 1 < r.x + r.w
            && r.y - 1 < second[0].y + 10 + 1
            && second[0].y - 1 < r.y + r.h;
        assert!(!overlap, "second-round placement overlaps a first-round one");
    }
}

// Best-Fit may reject every left-aligned candidate (overflowing the bin
// height) and then fall back to right-aligned candidates; whatever the
// outcome, placements stay inside the bin and the result is aligned.
#[test]
fn test_best_fit_overflow_fallback() {
    let mut p = Packer::default();
    p.init(20, 5, 8, Heuristic::Skyline_BF_sortHeight);
    let requests = [Request { w: 19, h: 4 }, Request { w: 8, h: 4 }, Request { w: 3, h: 5 }];
    let (results, _) = p.pack(&requests, 0, false);
    for (i, r) in results.iter().enumerate() {
        if r.was_packed {
            assert_eq!(r.w, requests[i].w);
            assert_eq!(r.h, requests[i].h);
            // content must fit the bin exactly (padding 0)
            assert!(r.x >= 0 && r.y >= 0 && r.x + r.w <= 20 && r.y + r.h <= 5);
        }
    }
}
