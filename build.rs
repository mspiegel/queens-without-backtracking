// Build-time polyomino enumerator.
//
// This build script enumerates every free polyomino of size 2 through 6,
// computes each shape's always-dead cell offsets, and emits a generated
// Rust source file at $OUT_DIR/polyomino_table.rs which src/polyomino.rs
// includes via the include! macro.
//
// The enumeration primitives live in src/polyomino_enum.rs so they can be
// unit-tested by the main crate via cargo test. This file re-includes that
// source so build.rs can call the same functions.

include!("src/polyomino_enum.rs");

use std::io::Write;

/// Hand-maintained roster of named polyomino shapes that carry a
/// polyomino-specific deduction. Each entry is `(counter_name, one_sample_of_the_shape)`.
/// The build script canonicalizes each sample, verifies it sits in the
/// enumerated set of free polyominoes of the correct size, and computes its
/// always-dead offsets.
const NAMED_SHAPES: &[(&str, &[(i8, i8)])] = &[
    // size 2: 1 shape
    ("polyomino_domino", &[(0, 0), (0, 1)]),
    // size 3: 2 shapes
    ("polyomino_i_tromino", &[(0, 0), (0, 1), (0, 2)]),
    ("polyomino_l_tromino", &[(0, 0), (0, 1), (1, 0)]),
    // size 4: 3 shapes that carry a deduction (I and O dropped)
    ("polyomino_t_tetromino", &[(0, 0), (0, 1), (0, 2), (1, 1)]),
    ("polyomino_l_tetromino", &[(0, 0), (1, 0), (2, 0), (2, 1)]),
    ("polyomino_s_tetromino", &[(0, 1), (0, 2), (1, 0), (1, 1)]),
    // size 5: 7 shapes that carry a deduction (I, P, X, Y, Z dropped)
    ("polyomino_l_pentomino", &[(0, 0), (1, 0), (2, 0), (3, 0), (3, 1)]),
    ("polyomino_f_pentomino", &[(0, 1), (0, 2), (1, 0), (1, 1), (2, 1)]),
    ("polyomino_n_pentomino", &[(0, 1), (1, 1), (2, 0), (2, 1), (3, 0)]),
    ("polyomino_t_pentomino", &[(0, 0), (0, 1), (0, 2), (1, 1), (2, 1)]),
    ("polyomino_u_pentomino", &[(0, 0), (0, 2), (1, 0), (1, 1), (1, 2)]),
    ("polyomino_v_pentomino", &[(0, 0), (1, 0), (2, 0), (2, 1), (2, 2)]),
    ("polyomino_w_pentomino", &[(0, 0), (1, 0), (1, 1), (2, 1), (2, 2)]),
    // size 6: 11 shapes that carry a deduction
    (
        "polyomino_z_with_tab_hexomino",
        &[(0, 0), (0, 1), (0, 2), (1, 0), (1, 2), (1, 3)],
    ),
    (
        "polyomino_l_hexomino",
        &[(0, 0), (0, 1), (0, 2), (0, 3), (0, 4), (1, 0)],
    ),
    (
        "polyomino_n_hexomino",
        &[(0, 0), (0, 1), (0, 2), (0, 3), (1, 3), (1, 4)],
    ),
    (
        "polyomino_long_n_hexomino",
        &[(0, 0), (0, 1), (0, 2), (1, 2), (1, 3), (1, 4)],
    ),
    (
        "polyomino_c_hexomino",
        &[(0, 0), (0, 1), (0, 2), (1, 0), (1, 2), (2, 0)],
    ),
    (
        "polyomino_s_block_hexomino",
        &[(0, 0), (0, 1), (1, 1), (1, 2), (2, 0), (2, 1)],
    ),
    (
        "polyomino_f_ext_a_hexomino",
        &[(0, 0), (0, 1), (0, 2), (1, 2), (1, 3), (2, 2)],
    ),
    (
        "polyomino_n_ext_a_hexomino",
        &[(0, 0), (0, 1), (0, 2), (1, 2), (1, 3), (2, 3)],
    ),
    (
        "polyomino_t_ext_hexomino",
        &[(0, 0), (0, 1), (1, 1), (1, 2), (1, 3), (2, 1)],
    ),
    (
        "polyomino_w_ext_hexomino",
        &[(0, 0), (0, 1), (1, 1), (1, 2), (1, 3), (2, 2)],
    ),
    (
        "polyomino_t_hexomino",
        &[(0, 0), (0, 1), (0, 2), (1, 1), (2, 1), (3, 1)],
    ),
];

fn main() {
    // Enumerate all free polyominoes of size 2..=6 and index them by the
    // canonical form. This gives us a set-level integrity check against the
    // NAMED_SHAPES roster.
    let mut enumerated_by_size: Vec<std::collections::BTreeSet<Poly>> = Vec::new();
    let mut current = monomino();
    for _ in 1..=5 {
        current = grow(&current);
        enumerated_by_size.push(current.clone());
    }
    let size_counts: Vec<usize> = enumerated_by_size.iter().map(|s| s.len()).collect();
    assert_eq!(size_counts, vec![1, 2, 5, 12, 35]);

    // For every shape in the enumerated sets that has at least one
    // polyomino-specific dead offset, we expect to find a matching entry in
    // NAMED_SHAPES. We build a set of canonical forms that surfaced as named
    // so we can spot any that slipped through.
    let mut seen_named_canons: std::collections::BTreeSet<Poly> =
        std::collections::BTreeSet::new();

    // Canonicalize each named sample and compute its dead offsets.
    let mut entries: Vec<(String, Poly, Vec<(i8, i8)>)> = Vec::with_capacity(NAMED_SHAPES.len());
    for (name, sample) in NAMED_SHAPES {
        let canon = canonical(sample);
        let size = canon.len();
        assert!(
            (2..=6).contains(&size),
            "named shape {name} has unsupported size {size}"
        );
        let enum_set = &enumerated_by_size[size - 2];
        assert!(
            enum_set.contains(&canon),
            "named shape {name} canonical form not found in free-polyomino enumeration"
        );
        let offsets = always_dead_offsets(&canon);
        assert!(
            !offsets.is_empty(),
            "named shape {name} has no polyomino-specific dead offsets"
        );
        assert!(
            seen_named_canons.insert(canon.clone()),
            "named shape {name} has a duplicate canonical form"
        );
        entries.push(((*name).to_string(), canon, offsets));
    }

    // Sanity check: every enumerated shape with a non-empty dead-offset set
    // must appear in NAMED_SHAPES. If the enumerator ever surfaces a new
    // shape with a deduction, the assertion fires so the discrepancy is
    // surfaced at build time rather than silently ignored.
    let mut expected_positive = 0;
    for enum_set in &enumerated_by_size {
        for shape in enum_set {
            if !always_dead_offsets(shape).is_empty() {
                expected_positive += 1;
                assert!(
                    seen_named_canons.contains(shape),
                    "enumerated shape {:?} has a polyomino-specific deduction but is not named in NAMED_SHAPES",
                    shape
                );
            }
        }
    }
    assert_eq!(
        expected_positive,
        NAMED_SHAPES.len(),
        "NAMED_SHAPES roster size ({}) disagrees with enumerator ({})",
        NAMED_SHAPES.len(),
        expected_positive
    );

    // Emit the generated table.
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR missing");
    let out_path = std::path::Path::new(&out_dir).join("polyomino_table.rs");
    let mut f = std::fs::File::create(&out_path).expect("create polyomino_table.rs");
    writeln!(
        f,
        "// Generated by build.rs. Do not edit."
    )
    .unwrap();
    writeln!(f).unwrap();
    writeln!(
        f,
        "pub static POLYOMINO_SHAPE_RULES: &[Shape] = &["
    )
    .unwrap();
    for (name, canon, offsets) in &entries {
        writeln!(f, "    Shape {{").unwrap();
        writeln!(f, "        name: {name:?},").unwrap();
        write!(f, "        cells: &[").unwrap();
        for (i, (r, c)) in canon.iter().enumerate() {
            if i > 0 {
                write!(f, ", ").unwrap();
            }
            write!(f, "({r}, {c})").unwrap();
        }
        writeln!(f, "],").unwrap();
        write!(f, "        dead_offsets: &[").unwrap();
        for (i, (r, c)) in offsets.iter().enumerate() {
            if i > 0 {
                write!(f, ", ").unwrap();
            }
            write!(f, "({r}, {c})").unwrap();
        }
        writeln!(f, "],").unwrap();
        writeln!(f, "    }},").unwrap();
    }
    writeln!(f, "];").unwrap();

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/polyomino_enum.rs");
}
