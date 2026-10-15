//! Size checks for the M1 exit criteria: full-text search under 100 ms and
//! a usable graph on a vault of thousands of notes. Ignored by default
//! because it writes 10k files; run with
//! `cargo test -p graphite-core --release --test perf -- --ignored --nocapture`.

use std::fs;
use std::time::Instant;

use graphite_core::Vault;

const WORDS: &[&str] = &[
    "motor",
    "driver",
    "battery",
    "chassis",
    "pump",
    "sensor",
    "garden",
    "robot",
    "wheel",
    "frame",
    "lidar",
    "camera",
    "firmware",
    "bracket",
    "valve",
    "solar",
    "charger",
    "controller",
    "gearbox",
    "bearing",
    "antenna",
    "relay",
    "fuse",
    "hinge",
    "bolt",
    "weld",
    "paint",
    "sketch",
    "budget",
    "supplier",
    "shipping",
    "review",
    "test",
    "plan",
    "idea",
];

fn body(i: usize, n: usize) -> String {
    let w = |k: usize| WORDS[(i * 7 + k * 13) % WORDS.len()];
    let link = |k: usize| format!("[[Note {}]]", (i * 31 + k * 17) % n);
    format!(
        "# Note {i}\n\nThe {} {} needs a {} before the {} is done. {} and {}.\n\ndepends-on:: {}\n\n- [ ] check the {}\n- [x] order the {}\n",
        w(0),
        w(1),
        w(2),
        w(3),
        link(0),
        link(1),
        link(2),
        w(4),
        w(5)
    )
}

#[test]
#[ignore]
fn ten_thousand_notes() {
    let n: usize = std::env::var("GRAPHITE_PERF_NOTES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000);
    let dir = tempfile::tempdir().unwrap();
    let notes = dir.path().join("notes");
    fs::create_dir_all(&notes).unwrap();
    let t = Instant::now();
    for i in 0..n {
        let tag = WORDS[i % WORDS.len()];
        fs::write(
            notes.join(format!("Note {i}.md")),
            format!(
                "---\ntype: {}\ntags: [{tag}]\n---\n{}",
                if i % 9 == 0 { "project" } else { "note" },
                body(i, n)
            ),
        )
        .unwrap();
    }
    eprintln!("wrote {n} notes in {:?}", t.elapsed());

    let t = Instant::now();
    let v = Vault::open(dir.path()).unwrap();
    let open = t.elapsed();
    eprintln!("open + first index: {open:?}");

    let t = Instant::now();
    let v2 = Vault::open(dir.path()).unwrap();
    eprintln!("reopen (no changes): {:?}", t.elapsed());
    drop(v2);

    // Same files, ids already minted, fresh index: parse + index cost alone.
    let dir2 = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir2.path().join("notes")).unwrap();
    for e in fs::read_dir(&notes).unwrap() {
        let e = e.unwrap();
        fs::copy(e.path(), dir2.path().join("notes").join(e.file_name())).unwrap();
    }
    let t = Instant::now();
    let v3 = Vault::open(dir2.path()).unwrap();
    eprintln!("first index with ids present: {:?}", t.elapsed());
    drop(v3);

    let t = Instant::now();
    let list = v.list().unwrap();
    eprintln!("list {} notes: {:?}", list.len(), t.elapsed());

    for q in ["motor", "mot", "battery charger", "note 4242", "lidar bracket plan"] {
        let t = Instant::now();
        let hits = v.search(q, 30).unwrap();
        let took = t.elapsed();
        eprintln!("search {q:?}: {} hits in {took:?}", hits.len());
        assert!(took.as_millis() < 100, "search {q:?} took {took:?}");
    }

    let t = Instant::now();
    let g = v.graph().unwrap();
    let took = t.elapsed();
    eprintln!("graph: {} nodes, {} edges in {took:?}", g.nodes.len(), g.edges.len());
    assert!(took.as_millis() < 1500, "graph took {took:?}");

    let t = Instant::now();
    let local = v.local_graph(&list[0].id, 2).unwrap();
    eprintln!(
        "local graph (2 hops): {} nodes, {} edges in {:?}",
        local.nodes.len(),
        local.edges.len(),
        t.elapsed()
    );

    let t = Instant::now();
    let b = v.backlinks(&list[0].id).unwrap();
    eprintln!("backlinks: {} in {:?}", b.len(), t.elapsed());

    let t = Instant::now();
    let c = v.complete_title("Note 12", 12).unwrap();
    eprintln!("complete_title: {} in {:?}", c.len(), t.elapsed());
}
