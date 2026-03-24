use crate::act::ActFile;

const PLAYER_BASES: &[(usize, &str)] = &[
    (0, "stand"),
    (8, "walk"),
    (16, "sit"),
    (24, "pickup"),
    (32, "atk_wait"),
    (40, "attack"),
    (48, "damage"),
    (56, "damage2"),
    (64, "dead"),
    (72, "unk"),
    (80, "attack2"),
    (88, "attack3"),
    (96, "skill"),
];

const MONSTER_BASES: &[(usize, &str)] = &[
    (0, "stand"),
    (8, "move"),
    (16, "attack"),
    (24, "damage"),
    (32, "dead"),
];

const DIRS: &[&str] = &["s", "sw", "w", "nw", "n", "ne", "e", "se"];

fn action_label(idx: usize, total_actions: usize) -> String {
    let base = idx - (idx % 8);
    let dir = idx % 8;
    // Use monster labels when action count is clearly not a player sprite (not 104)
    let bases: &[(usize, &str)] = if total_actions != 104 && total_actions.is_multiple_of(8)
        && total_actions <= 40
    {
        MONSTER_BASES
    } else {
        PLAYER_BASES
    };
    if let Some(&(_, name)) = bases.iter().find(|&&(b, _)| b == base) {
        format!("{}_{}", name, DIRS[dir])
    } else {
        format!("action_{idx:03}_{}", DIRS[dir])
    }
}

fn has_visible_sprite(act: &ActFile, action_idx: usize) -> bool {
    let action = &act.actions[action_idx];
    action.frames.iter().any(|f| {
        f.sprites.iter().any(|s| s.spr_id >= 0 && s.x_scale.abs() > 1e-6)
    })
}

/// Print which action indices have at least one visible sprite layer.
pub fn scan(act: &ActFile) {
    println!(
        "ACT v{:#06x}: {} actions, {} events",
        act.version,
        act.actions.len(),
        act.events.len()
    );
    println!();
    println!("Actions with visible sprites:");

    let mut last_base: Option<usize> = None;
    for i in 0..act.actions.len() {
        if !has_visible_sprite(act, i) {
            continue;
        }
        let base = i - (i % 8);
        if last_base != Some(base) {
            println!();
            last_base = Some(base);
        }
        let n_frames = act.actions[i].frames.len();
        println!("  {:3}  {}  ({} frames)", i, action_label(i, act.actions.len()), n_frames);
    }
}

/// Dump per-frame layer and attach-point data for the given actions.
/// If `action_filter` is None, dumps all actions that have visible sprites.
pub fn dump(act: &ActFile, action_filter: Option<&[usize]>) {
    println!(
        "ACT v{:#06x}: {} actions, {} events",
        act.version,
        act.actions.len(),
        act.events.len()
    );

    if !act.events.is_empty() {
        println!("Events:");
        for (i, e) in act.events.iter().enumerate() {
            println!("  [{i}] {e:?}");
        }
    }

    let indices: Vec<usize> = match action_filter {
        Some(f) => f.iter().copied().filter(|&i| i < act.actions.len()).collect(),
        None => (0..act.actions.len())
            .filter(|&i| has_visible_sprite(act, i))
            .collect(),
    };

    for action_idx in indices {
        let action = &act.actions[action_idx];
        println!(
            "\n=== action {:3}  {}  ({} frames, interval={:.0}ms) ===",
            action_idx,
            action_label(action_idx, act.actions.len()),
            action.frames.len(),
            action.frame_ms(),
        );

        for (fi, frame) in action.frames.iter().enumerate() {
            let event = if frame.event_id >= 0 {
                format!(
                    "  event={} ({:?})",
                    frame.event_id,
                    act.events.get(frame.event_id as usize).map(|s| s.as_str()).unwrap_or("?")
                )
            } else {
                String::new()
            };

            let attach_str = if frame.attach_points.is_empty() {
                String::new()
            } else {
                let pts: Vec<String> = frame
                    .attach_points
                    .iter()
                    .map(|p| format!("({},{})", p.x, p.y))
                    .collect();
                format!("  attach=[{}]", pts.join(", "))
            };

            println!("  frame {:2}:{}{}", fi, event, attach_str);

            for (si, s) in frame.sprites.iter().enumerate() {
                let flip = if s.flags & 1 != 0 { " flip" } else { "" };
                let scale = if (s.x_scale - s.y_scale).abs() < 1e-4 {
                    format!("{:.3}", s.x_scale)
                } else {
                    format!("{:.3}x{:.3}", s.x_scale, s.y_scale)
                };
                let rot = if s.rotation != 0 {
                    format!(" rot={}", s.rotation)
                } else {
                    String::new()
                };
                let tint = if s.tint != [255, 255, 255, 255] {
                    format!(" tint={:?}", s.tint)
                } else {
                    String::new()
                };
                println!(
                    "    layer {:2}: spr_id={:4} type={} x={:4} y={:4} scale={}{}{}{} ",
                    si, s.spr_id, s.spr_type, s.x, s.y, scale, rot, flip, tint
                );
            }
        }
    }
}
