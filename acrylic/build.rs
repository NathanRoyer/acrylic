use railway::{*, computing::{*, Operation::*}};
use std::{fs, path::Path, env};
use core::f32::consts::FRAC_PI_2;
use rand::random;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let out_dir = env::var("OUT_DIR").unwrap();

    let cont_rwy_dst = Path::new(&out_dir).join("container.rwy");
    fs::write(cont_rwy_dst, &gen_container_rwy()).unwrap();

    let rnd_seed_dst = Path::new(&out_dir).join("seed.dat");
    let rnd_seed: [u8; 32] = random();
    fs::write(rnd_seed_dst, &rnd_seed).unwrap();
}

fn gen_container_rwy() -> Vec<u8> {
    let mut arguments = Vec::new();

    let zero = arguments.len();
    arguments.push(Argument::unnamed(C_ZERO));

    let top_left = zero;

    let size = arguments.len();
    arguments.push(Argument::named("size", Couple::new(400.0, 400.0)));

    let bottom_right = size;

    let margin_radius = arguments.len();
    arguments.push(Argument::named("margin-radius", Couple::new(30.0, 30.0)));

    let border_rg = arguments.len();
    arguments.push(Argument::named("border-rg", Couple::new(1.0, 1.0)));
    let border_ba = arguments.len();
    arguments.push(Argument::named("border-ba", Couple::new(1.0, 1.0)));
    let border_width = arguments.len();
    arguments.push(Argument::named("border-width", Couple::new(2.0, 0.0)));

    let ext_rg = arguments.len();
    arguments.push(Argument::named("ext-rg", Couple::new(0.0, 0.0)));
    let ext_ba = arguments.len();
    arguments.push(Argument::named("ext-ba", Couple::new(0.0, 1.0)));

    let solid_line = arguments.len();
    arguments.push(Argument::unnamed(Couple::new(100.0, 0.0)));

    let arc_deltas = arguments.len();
    arguments.push(Argument::unnamed(Couple::new(-FRAC_PI_2, 0.0)));

    let mut instructions = Vec::new();

    let margin = arguments.len() + instructions.len();
    instructions.push(Instruction::new(EachX2, margin_radius, margin_radius, 0));

    let nw = arguments.len() + instructions.len();
    instructions.push(Instruction::new(Add2, top_left, margin, 0));

    let se = arguments.len() + instructions.len();
    instructions.push(Instruction::new(Subtract2, bottom_right, margin, 0));

    let radius = arguments.len() + instructions.len();
    instructions.push(Instruction::new(EachY2, margin_radius, margin_radius, 0));

    let nw_c = arguments.len() + instructions.len();
    instructions.push(Instruction::new(Add2, nw, radius, 0));

    let se_c = arguments.len() + instructions.len();
    instructions.push(Instruction::new(Subtract2, se, radius, 0));

    let bottom_left = arguments.len() + instructions.len();
    instructions.push(Instruction::new(Select2, top_left, bottom_right, 0));

    let top_right = arguments.len() + instructions.len();
    instructions.push(Instruction::new(Select2, bottom_right, top_left, 0));

    let sw = arguments.len() + instructions.len();
    instructions.push(Instruction::new(Select2, nw, se, 0));

    let ne = arguments.len() + instructions.len();
    instructions.push(Instruction::new(Select2, se, nw, 0));

    let sw_c = arguments.len() + instructions.len();
    instructions.push(Instruction::new(Select2, nw_c, se_c, 0));

    let ne_c = arguments.len() + instructions.len();
    instructions.push(Instruction::new(Select2, se_c, nw_c, 0));

    let jump = arguments.len() + instructions.len();
    instructions.push(Instruction::new(Select2, top_left, nw_c, 0));

    let a0_in = arguments.len() + instructions.len();
    instructions.push(Instruction::new(Select2, nw, nw_c, 0));

    // let a0_out = arguments.len() + instructions.len();
    // instructions.push(Instruction::new(Select2, nw_c, nw, 0));

    let a1_in = arguments.len() + instructions.len();
    instructions.push(Instruction::new(Select2, ne_c, ne, 0));

    // let a1_out = arguments.len() + instructions.len();
    // instructions.push(Instruction::new(Select2, ne, ne_c, 0));

    let a2_in = arguments.len() + instructions.len();
    instructions.push(Instruction::new(Select2, se, se_c, 0));

    // let a2_out = arguments.len() + instructions.len();
    // instructions.push(Instruction::new(Select2, se_c, se, 0));

    let a3_in = arguments.len() + instructions.len();
    instructions.push(Instruction::new(Select2, sw_c, sw, 0));

    let a3_out = arguments.len() + instructions.len();
    instructions.push(Instruction::new(Select2, sw, sw_c, 0));

    let border_color = [border_rg, border_ba];
    let ext_color = [ext_rg, ext_ba];

    let line_style = Stroker {
        pattern: solid_line,
        width: border_width,
        color: border_color,
    };

    let background = vec![
        Triangle {
            points: [top_left, bottom_left, bottom_right],
            colors: [ext_color, ext_color, ext_color],
        },
        Triangle {
            points: [top_left, top_right, bottom_right],
            colors: [ext_color, ext_color, ext_color],
        },
    ];

    let path = vec![
        PathStep::Line(Line {
            points: [jump, bottom_left],
        }),
        PathStep::Line(Line {
            points: [bottom_left, bottom_right],
        }),
        PathStep::Line(Line {
            points: [bottom_right, top_right],
        }),
        PathStep::Line(Line {
            points: [top_right, top_left],
        }),
        PathStep::Line(Line {
            points: [top_left, jump],
        }),
        PathStep::Arc(Arc {
            start_point: a0_in,
            center: nw_c,
            deltas: arc_deltas,
        }),
        /*PathStep::Line(Line {
            points: [a0_out, a1_in],
        }),*/
        PathStep::Arc(Arc {
            start_point: a1_in,
            center: ne_c,
            deltas: arc_deltas,
        }),
        /*PathStep::Line(Line {
            points: [a1_out, a2_in],
        }),*/
        PathStep::Arc(Arc {
            start_point: a2_in,
            center: se_c,
            deltas: arc_deltas,
        }),
        /*PathStep::Line(Line {
            points: [a2_out, a3_in],
        }),*/
        PathStep::Arc(Arc {
            start_point: a3_in,
            center: sw_c,
            deltas: arc_deltas,
        }),
        PathStep::Line(Line {
            points: [a3_out, a0_in],
        }),
        PathStep::Line(Line {
            points: [a0_in, jump],
        }),
        PathStep::Line(Line {
            points: [jump, top_left],
        }),
    ];

    let border_path = vec![
        PathStep::Arc(Arc {
            start_point: a0_in,
            center: nw_c,
            deltas: arc_deltas,
        }),
        /*PathStep::Line(Line {
            points: [a0_out, a1_in],
        }),*/
        PathStep::Arc(Arc {
            start_point: a1_in,
            center: ne_c,
            deltas: arc_deltas,
        }),
        /*PathStep::Line(Line {
            points: [a1_out, a2_in],
        }),*/
        PathStep::Arc(Arc {
            start_point: a2_in,
            center: se_c,
            deltas: arc_deltas,
        }),
        /*PathStep::Line(Line {
            points: [a2_out, a3_in],
        }),*/
        PathStep::Arc(Arc {
            start_point: a3_in,
            center: sw_c,
            deltas: arc_deltas,
        }),
    ];

    let rendering_steps: [RenderingStep<&Vec<PathStep>, &Vec<Triangle>>; 2] = [
        RenderingStep::Clip(&path, &background),
        RenderingStep::Stroke(&border_path, line_style),
    ];

    serialize(&arguments, &instructions, &[], &rendering_steps)
}
