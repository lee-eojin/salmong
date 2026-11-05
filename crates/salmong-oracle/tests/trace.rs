use salmong_core::ids::{BusId, Dir, PassengerId, StationId, Tick};
use salmong_core::scenario::Shape;
use salmong_core::trace::{Action, BusState, Phase, TraceState, TraceStep};
use salmong_oracle::trace::{Corruption, balance, corrupt, sample, tla_module};

fn coherent(steps: &[TraceStep]) -> bool {
    steps.windows(2).all(|w| w[0].after == w[1].before)
}

fn kinds(steps: &[TraceStep]) -> Vec<&'static str> {
    let mut v = steps
        .iter()
        .map(|s| match s.action {
            Action::Spawn { .. } => "Spawn",
            Action::EditNetwork => "EditNetwork",
            Action::Move { .. } => "Move",
            Action::Alight { .. } => "Alight",
            Action::EndAlight { .. } => "EndAlight",
            Action::Board { .. } => "Board",
            Action::Depart { .. } => "Depart",
        })
        .collect::<Vec<_>>();
    v.sort();
    v.dedup();
    v
}

#[test]
fn samples_are_deterministic_coherent_and_cover_every_action() {
    for seed in 1..=4 {
        let steps = sample(seed);
        assert_eq!(steps, sample(seed));
        assert!(steps.len() > 300, "seed {seed}: {}", steps.len());
        assert!(coherent(&steps));
        assert_eq!(
            kinds(&steps),
            [
                "Alight",
                "Board",
                "Depart",
                "EditNetwork",
                "EndAlight",
                "Move",
                "Spawn"
            ]
        );
        let last = &steps.last().unwrap().after;
        let edits = steps
            .iter()
            .filter(|s| s.action == Action::EditNetwork)
            .count();
        assert_eq!(last.edits as usize, edits);
        assert_eq!(last.buses.len(), 2);
        let removed = seed % 2 == 1;
        assert_eq!(last.network[1].is_empty(), removed);
        assert_eq!(last.buses[1].retiring, removed);
        assert_eq!(last.buses[1].phase == Phase::Parked, removed);
        assert!(
            last.buses
                .iter()
                .all(|b| b.riders.len() <= balance().capacity)
        );
    }
    assert_ne!(sample(1), sample(2));
}

#[test]
fn corruptions_stay_coherent_and_change_one_place() {
    let steps = sample(2);
    let k = steps.len() / 2;
    let dropped = corrupt(&steps, Corruption::Drop);
    assert_eq!(dropped.len(), steps.len() - 1);
    assert!(coherent(&dropped));
    assert_eq!(dropped[..k], steps[..k]);
    assert_eq!(dropped[k].action, steps[k + 1].action);
    assert_eq!(dropped[k].before, steps[k - 1].after);
    assert_eq!(dropped[k + 1..], steps[k + 2..]);

    let swapped = corrupt(&steps, Corruption::Swap);
    assert_eq!(swapped.len(), steps.len());
    assert!(coherent(&swapped));
    assert_eq!(swapped[k].action, steps[k + 1].action);
    assert_eq!(swapped[k + 1].action, steps[k].action);
    assert_eq!(swapped[k].after, steps[k + 1].after);
    assert_eq!(swapped[k + 1].after, steps[k].after);
    assert_eq!(swapped[k + 3..], steps[k + 3..]);

    let altered = corrupt(&steps, Corruption::Alter);
    assert_eq!(altered.len(), steps.len());
    assert!(coherent(&altered));
    let diff = (0..steps.len())
        .filter(|&i| altered[i].after != steps[i].after)
        .collect::<Vec<_>>();
    assert_eq!(diff.len(), 1);
    let i = diff[0];
    assert!(i >= k);
    assert!(matches!(steps[i].action, Action::Spawn { .. }));
    assert!(
        steps[k..i]
            .iter()
            .all(|s| !matches!(s.action, Action::Spawn { .. }))
    );
    assert_eq!(
        altered[i].after.buses[0].dir,
        steps[i].after.buses[0].dir.reverse()
    );
    assert_eq!(altered[i].after.buses[1], steps[i].after.buses[1]);
    assert!(corrupt(&[], Corruption::Drop).is_empty());
}

#[test]
fn alter_without_a_later_spawn_flips_the_middle_step() {
    let steps = sample(2);
    let k = steps.len() / 2;
    let tail = steps
        .iter()
        .rposition(|s| matches!(s.action, Action::Spawn { .. }))
        .unwrap();
    let cut = &steps[..tail];
    let k2 = cut.len() / 2;
    let altered = corrupt(cut, Corruption::Alter);
    assert!(k2 < k);
    let later = cut[k2..]
        .iter()
        .position(|s| matches!(s.action, Action::Spawn { .. }));
    let i = later.map_or(k2, |j| k2 + j);
    assert_eq!(
        altered[i].after.buses[0].dir,
        cut[i].after.buses[0].dir.reverse()
    );
    let mid = &steps[..2];
    let no_spawn = mid
        .iter()
        .map(|s| TraceStep {
            action: Action::Move { bus: BusId(0) },
            ..s.clone()
        })
        .collect::<Vec<_>>();
    let altered = corrupt(&no_spawn, Corruption::Alter);
    assert_eq!(
        altered[1].after.buses[0].dir,
        no_spawn[1].after.buses[0].dir.reverse()
    );
    assert_eq!(altered[0].after, no_spawn[0].after);
}

fn tiny_state(spawned: bool) -> TraceState {
    TraceState {
        network: vec![vec![StationId(0), StationId(1), StationId(2)], vec![]],
        buses: vec![
            BusState {
                at: StationId(1),
                dir: Dir::Fwd,
                phase: Phase::Moving,
                retiring: false,
                riders: vec![PassengerId(0)],
            },
            BusState {
                at: StationId(3),
                dir: Dir::Back,
                phase: Phase::Parked,
                retiring: true,
                riders: vec![],
            },
        ],
        queues: vec![vec![], vec![], vec![], vec![]],
        delivered: if spawned {
            vec![PassengerId(1)]
        } else {
            vec![]
        },
        edits: 1,
        wants: if spawned {
            vec![Shape::Square, Shape::Triangle]
        } else {
            vec![Shape::Square]
        },
    }
}

#[test]
fn tla_module_renders_one_based_records() {
    let step = TraceStep {
        tick: Tick(7),
        action: Action::Board {
            bus: BusId(1),
            passenger: PassengerId(4),
        },
        before: tiny_state(false),
        after: tiny_state(true),
    };
    let other = TraceStep {
        action: Action::Alight {
            bus: BusId(0),
            passenger: PassengerId(0),
        },
        ..step.clone()
    };
    let s = |spawned: &str, delivered: &str, want: &str| {
        format!(
            "[network |-> <<<<1, 2, 3>>, <<>>>>, dir |-> <<\"Fwd\", \"Back\">>, retiring |-> {{2}}, queue |-> <<<<>>, <<>>, <<>>, <<>>>>, onboard |-> <<<<1>>, <<>>>>, busAt |-> <<2, 4>>, busPhase |-> <<\"Moving\", \"Parked\">>, delivered |-> {delivered}, edits |-> 1, spawned |-> {spawned}, want |-> {want}]"
        )
    };
    let before = s("1", "{}", "<<\"Square\", \"Unset\">>");
    let after = s("2", "{2}", "<<\"Square\", \"Triangle\">>");
    let expected = format!(
        "---- MODULE BoardingTraceData ----\nEXTENDS Naturals, Sequences\nTraceCapacity == 2\nTraceMaxPassengers == 2\nTraceMaxEdits == 1\nTrace == <<\n  [tick |-> 7, act |-> \"Board\", bus |-> 2, p |-> 5,\n   before |-> {before},\n   after |-> {after}],\n  [tick |-> 7, act |-> \"Alight\", bus |-> 1, p |-> 1,\n   before |-> {before},\n   after |-> {after}]\n>>\n====\n"
    );
    assert_eq!(tla_module(&[step, other], 2), expected);
}

#[test]
fn tla_module_labels_every_action() {
    let base = sample(1);
    let text = tla_module(&base, 2);
    for (needle, count) in [
        ("act |-> \"Spawn\", bus |-> 0, p |-> 1,", 1),
        ("act |-> \"EditNetwork\", bus |-> 0, p |-> 0,", 1),
        ("act |-> \"Move\", bus |-> 1, p |-> 0,", 1),
        ("act |-> \"EndAlight\", bus |-> 2, p |-> 0,", 1),
        ("act |-> \"Depart\", bus |-> 2, p |-> 0,", 1),
        ("\"Alighting\"", 1),
        ("\"Boarding\"", 1),
    ] {
        assert!(text.matches(needle).count() >= count, "{needle}");
    }
    for bad in [
        "act |-> \"Spawn\", bus |-> 0, p |-> 0,",
        "act |-> \"Move\", bus |-> 0,",
        "act |-> \"EndAlight\", bus |-> 0,",
        "act |-> \"Depart\", bus |-> 0,",
        "act |-> \"Board\", bus |-> 0,",
        "act |-> \"Alight\", bus |-> 0,",
        "act |-> \"Board\", bus |-> 1, p |-> 0,",
        "act |-> \"Alight\", bus |-> 1, p |-> 0,",
    ] {
        assert!(!text.contains(bad), "{bad}");
    }
    let edits = base.last().unwrap().after.edits;
    assert!(text.contains(&format!("TraceMaxEdits == {edits}\n")));
    let spawned = base.last().unwrap().after.wants.len();
    assert!(text.contains(&format!("TraceMaxPassengers == {spawned}\n")));
    assert!(tla_module(&[], 2).contains("TraceMaxPassengers == 0\nTraceMaxEdits == 0\n"));
}

fn network(i: Option<usize>) -> Vec<Vec<StationId>> {
    let ids = |v: &[u16]| v.iter().map(|&s| StationId(s)).collect::<Vec<_>>();
    match i {
        Some(i) => salmong_oracle::trace::NETWORKS[i]
            .iter()
            .map(|r| ids(r))
            .collect(),
        None => vec![ids(&[0, 1, 2]), vec![]],
    }
}

#[test]
fn edit_schedule_matches_independent_pcg_derivation() {
    let table: [(u64, &[(u64, Option<usize>)]); 2] = [
        (
            1,
            &[
                (10, Some(2)),
                (24, Some(3)),
                (65, Some(2)),
                (75, Some(1)),
                (93, Some(3)),
                (112, Some(1)),
                (125, Some(0)),
                (127, Some(3)),
                (130, Some(2)),
                (135, Some(1)),
                (150, None),
            ],
        ),
        (
            2,
            &[
                (1, Some(2)),
                (9, Some(3)),
                (16, Some(2)),
                (49, Some(3)),
                (51, Some(0)),
                (52, Some(1)),
                (62, Some(3)),
                (68, Some(0)),
                (113, Some(1)),
                (128, Some(0)),
                (145, Some(3)),
                (153, Some(1)),
                (158, Some(3)),
                (163, Some(2)),
                (194, Some(0)),
                (199, Some(2)),
            ],
        ),
    ];
    for (seed, expected) in table {
        let steps = sample(seed);
        let edits = steps
            .iter()
            .filter(|s| s.action == Action::EditNetwork)
            .collect::<Vec<_>>();
        assert!(edits.iter().all(|s| s.before.network != s.after.network));
        let got = edits
            .iter()
            .map(|s| (s.tick.0, s.after.network.clone()))
            .collect::<Vec<_>>();
        let want = expected
            .iter()
            .map(|&(t, i)| (t, network(i)))
            .collect::<Vec<_>>();
        assert_eq!(got, want, "seed {seed}");
    }
}

fn oracle(args: &[&str]) -> (Option<i32>, String) {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_salmong-oracle"))
        .args(args)
        .output()
        .unwrap();
    (out.status.code(), String::from_utf8(out.stdout).unwrap())
}

#[test]
fn cli_prints_sample_and_corrupted_modules() {
    let steps = sample(3);
    let cap = balance().capacity;
    assert_eq!(oracle(&["trace", "3"]), (Some(0), tla_module(&steps, cap)));
    for (arg, kind) in [
        ("drop", Corruption::Drop),
        ("swap", Corruption::Swap),
        ("alter", Corruption::Alter),
    ] {
        let want = tla_module(&corrupt(&steps, kind), cap);
        assert_eq!(oracle(&["trace", "3", arg]), (Some(0), want), "{arg}");
    }
    for bad in [
        &["trace"][..],
        &["trace", "x"],
        &["trace", "3", "bogus"],
        &["trace", "3", "drop", "extra"],
    ] {
        assert_eq!(oracle(bad), (Some(2), String::new()), "{bad:?}");
    }
}

#[test]
fn cli_pack_and_unknown_command() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_salmong-oracle"))
        .args(["pack", "1", "1"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(out.stdout, salmong_oracle::pack_bytes(1, 1).unwrap());
    assert_eq!(oracle(&["bogus", "1", "1"]), (Some(2), String::new()));
    assert_eq!(oracle(&[]), (Some(2), String::new()));
}
