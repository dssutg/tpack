// guillotine.rs - Guillotine splitting packer (simple alternative strategy).
//
// A straightforward guillotine bin packer used as one strategy in "best" mode.
// Places rectangles in given order; splits remaining space with guillotine cuts.
// Not as good as skyline in general but can help in some cases.

#[derive(Clone, Debug)]
struct Node {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    used: bool,
}

#[derive(Clone, Debug)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub packed: bool,
}

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
            // split
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
