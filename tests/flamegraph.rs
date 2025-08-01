mod common;

use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Cursor};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::str::FromStr;

use assert_cmd::cargo::CommandCargoExt;
use clap::ValueEnum;
use inferno::flamegraph::color::{BackgroundColor, PaletteMap};
use inferno::flamegraph::{
    self, Direction, FrameWidthSource, Options, Palette, TextTruncateDirection,
};
use log::Level;
use pretty_assertions::assert_eq;
use testing_logger::CapturedLog;

fn test_flamegraph(
    input_file: &str,
    expected_result_file: &str,
    options: Options<'_>,
) -> io::Result<()> {
    test_flamegraph_multiple_files(
        vec![PathBuf::from_str(input_file).unwrap()],
        expected_result_file,
        options,
    )
}

fn test_flamegraph_multiple_files(
    input_files: Vec<PathBuf>,
    expected_result_file: &str,
    mut options: Options<'_>,
) -> io::Result<()> {
    // Always pretty print XML to make it easier to find differences when tests fail.
    options.pretty_xml = true;
    // Never include static JavaScript in tests so we don't have to have it duplicated
    // in all of the test files.
    options.no_javascript = true;

    let metadata = match fs::metadata(expected_result_file) {
        Ok(m) => m,
        Err(e) => {
            if e.kind() == io::ErrorKind::NotFound {
                // be nice to the dev and make the file
                let mut f = File::create(expected_result_file).unwrap();
                flamegraph::from_files(&mut options, &input_files, &mut f)?;
                fs::metadata(expected_result_file).unwrap()
            } else {
                return Err(e);
            }
        }
    };

    let expected_len = metadata.len() as usize;
    let mut result = Cursor::new(Vec::with_capacity(expected_len));
    flamegraph::from_files(&mut options, &input_files, &mut result)?;
    let expected = BufReader::new(File::open(expected_result_file).unwrap());
    // write out the expected result to /tmp for easy restoration
    result.set_position(0);
    let rand: u64 = rand::random();
    let tm = std::env::temp_dir().join(format!("test-{}.svg", rand));
    if fs::write(&tm, result.get_ref()).is_ok() {
        eprintln!("test output in {}", tm.display());
    }
    // and then compare
    result.set_position(0);
    if std::env::var("INFERNO_BLESS_TESTS").is_ok() {
        fs::write(expected_result_file, result.get_ref()).unwrap();
    } else {
        compare_results(result, expected, expected_result_file);
    }
    Ok(())
}

fn compare_results<R, E>(result: R, mut expected: E, expected_file: &str)
where
    R: BufRead,
    E: BufRead,
{
    const BLESS_MSG: &str = "If you have modified the code and specifically want to update the expected test output, set `INFERNO_BLESS_TESTS=1` before the `cargo test`.";
    let mut buf = String::new();
    let mut line_num = 1;
    for line in result.lines() {
        if expected.read_line(&mut buf).unwrap() == 0 {
            panic!(
                "\noutput has more lines than expected result file: {}\n\n{}",
                expected_file, BLESS_MSG
            );
        }
        assert_eq!(
            line.unwrap(),
            buf.trim_end(),
            "\n{}:{}\n\n{}",
            expected_file,
            line_num,
            BLESS_MSG
        );
        buf.clear();
        line_num += 1;
    }

    if expected.read_line(&mut buf).unwrap() > 0 {
        panic!(
            "\n{} has more lines than output, beginning at line: {}\n\n{}",
            expected_file, line_num, BLESS_MSG
        )
    }
}

fn test_flamegraph_logs<F>(input_file: &str, asserter: F)
where
    F: Fn(&Vec<CapturedLog>),
{
    test_flamegraph_logs_with_options(input_file, asserter, Default::default());
}

fn test_flamegraph_logs_with_options<F>(
    input_file: &str,
    asserter: F,
    mut options: flamegraph::Options<'_>,
) where
    F: Fn(&Vec<CapturedLog>),
{
    testing_logger::setup();
    let r = File::open(input_file).unwrap();
    let sink = io::sink();
    let _ = flamegraph::from_reader(&mut options, r, sink);
    testing_logger::validate(asserter);
}

#[test]
fn flamegraph_colors_deterministic() {
    let input_file = "./tests/data/flamegraph/colors/async-profiler-collapsed-part.txt";
    let expected_result_file = "./tests/data/flamegraph/colors/deterministic.svg";

    let mut options = flamegraph::Options::default();
    options.deterministic = true;

    test_flamegraph(input_file, expected_result_file, options).unwrap();
}

#[test]
fn flamegraph_colors_java() {
    let input_file = "./flamegraph/test/results/perf-java-stacks-01-collapsed-all.txt";
    let expected_result_file = "./tests/data/flamegraph/colors/java.svg";

    let mut options = flamegraph::Options::default();
    options.colors = Palette::from_str("java").unwrap();
    options.bgcolors = Some(BackgroundColor::from_str("blue").unwrap());
    options.hash = true;

    test_flamegraph(input_file, expected_result_file, options).unwrap();
}

#[test]
fn flamegraph_colors_java_async_profile() {
    let input_file = "./tests/data/flamegraph/colors/async-profiler-collapsed-part.txt";
    let expected_result_file = "./tests/data/flamegraph/colors/async-profiler-java.svg";

    let mut options = flamegraph::Options::default();
    options.colors = Palette::from_str("java").unwrap();
    options.hash = true;

    test_flamegraph(input_file, expected_result_file, options).unwrap();
}

#[test]
fn flamegraph_colors_js() {
    let input_file = "./flamegraph/test/results/perf-js-stacks-01-collapsed-all.txt";
    let expected_result_file = "./tests/data/flamegraph/colors/js.svg";

    let mut options = flamegraph::Options::default();
    options.colors = Palette::from_str("js").unwrap();
    options.bgcolors = Some(BackgroundColor::from_str("green").unwrap());
    options.hash = true;

    test_flamegraph(input_file, expected_result_file, options).unwrap();
}

#[test]
fn flamegraph_differential() {
    let input_file =
        "./tests/data/flamegraph/differential/perf-cycles-instructions-01-collapsed-all-diff.txt";
    let expected_result_file = "./tests/data/flamegraph/differential/diff.svg";
    test_flamegraph(input_file, expected_result_file, Default::default()).unwrap();
}

#[test]
fn flamegraph_differential_including_children() {
    let input_file =
        "./tests/data/flamegraph/differential/perf-cycles-instructions-01-collapsed-all-diff.txt";
    let expected_result_file = "./tests/data/flamegraph/differential/diff-including-children.svg";
    let mut options = flamegraph::Options::default();
    options.include_children = true;
    test_flamegraph(input_file, expected_result_file, options).unwrap();
}
#[test]
fn flamegraph_differential_including_children_color_normalization() {
    let input_file = "./tests/data/flamegraph/differential/stronger-parents-diff.txt";
    let expected_result_file = "./tests/data/flamegraph/differential/stronger-parents-diff.svg";
    let mut options = flamegraph::Options::default();
    options.include_children = true;
    test_flamegraph(input_file, expected_result_file, options).unwrap();
}
#[test]
fn flamegraph_differential_with_different_frame_width_sources() {
    let input_dir = "./tests/data/flamegraph/differential/";
    let input_file_bases = [
        "self-vs-total",
        "unbalanced-sampling_more-before",
        "unbalanced-sampling_more-after",
        "before-vs-after"
    ];
    input_file_bases.into_iter().for_each(|input_file_base| {
        let input_file = format!("{input_dir}{input_file_base}.txt");

        for width_source in FrameWidthSource::value_variants() {
            for include_children in [false, true].into_iter() {
                let expected_result_file = format!(
                    "{input_dir}{input_file_base}_width-from-{}_color-from-{}.svg",
                    width_source.to_possible_value().unwrap().get_name(),
                    if include_children { "total" } else { "self" }
                );
                let mut options = flamegraph::Options::default();
                options.frame_width_source = *width_source;
                options.include_children = include_children;
                options.detailed_tooltips = true;
                options.normalize = true;
                test_flamegraph(&input_file, &expected_result_file, options).unwrap();
            }
        }
    });
}
#[test]
fn flamegraph_differential_difference_widths() {
    let input_file = "./tests/data/flamegraph/differential/difference-widths.txt";
    let expected_result_file = "./tests/data/flamegraph/differential/difference-widths.svg";
    let mut options = flamegraph::Options::default();
    options.include_children = true;
    options.frame_width_source = FrameWidthSource::Difference;
    options.detailed_tooltips = true;
    test_flamegraph(input_file, expected_result_file, options).unwrap();
}

#[test]
fn flamegraph_differential_negated() {
    let input_file =
        "./tests/data/flamegraph/differential/perf-cycles-instructions-01-collapsed-all-diff.txt";
    let expected_result_file = "./tests/data/flamegraph/differential/diff-negated.svg";
    let mut options = flamegraph::Options::default();
    options.negate_differentials = true;
    test_flamegraph(input_file, expected_result_file, options).unwrap();
}

#[test]
fn flamegraph_collor_diffusion() {
    let input_file = "./flamegraph/test/results/perf-vertx-stacks-01-collapsed-all.txt";
    let expected_result_file = "./tests/data/flamegraph/options/colordiffusion.svg";
    let mut options = flamegraph::Options::default();
    options.color_diffusion = true;
    test_flamegraph(input_file, expected_result_file, options).unwrap();
}

#[test]
fn flamegraph_factor() {
    let input_file = "./flamegraph/test/results/perf-vertx-stacks-01-collapsed-all.txt";
    let expected_result_file = "./tests/data/flamegraph/factor/factor-2.5.svg";
    let mut options = flamegraph::Options::default();
    options.factor = 2.5;
    options.hash = true;
    test_flamegraph(input_file, expected_result_file, options).unwrap();
}

#[test]
#[cfg(feature = "nameattr")]
fn flamegraph_nameattr() {
    let input_file = "./flamegraph/test/results/perf-cycles-instructions-01-collapsed-all.txt";
    let expected_result_file = "./tests/data/flamegraph/nameattr/nameattr.svg";
    let nameattr_file = "./tests/data/flamegraph/nameattr/nameattr.txt";

    let mut options = flamegraph::Options::default();
    options.hash = true;
    options.func_frameattrs =
        flamegraph::FuncFrameAttrsMap::from_file(&PathBuf::from(nameattr_file)).unwrap();

    test_flamegraph(input_file, expected_result_file, options).unwrap();
}

#[test]
#[cfg(feature = "nameattr")]
fn flamegraph_nameattr_empty_line() {
    let input_file = "./flamegraph/test/results/perf-cycles-instructions-01-collapsed-all.txt";
    let expected_result_file = "./tests/data/flamegraph/nameattr/nameattr.svg";
    let nameattr_file = "./tests/data/flamegraph/nameattr/nameattr_empty_first_line.txt";

    let mut options = flamegraph::Options::default();
    options.hash = true;
    options.func_frameattrs =
        flamegraph::FuncFrameAttrsMap::from_file(&PathBuf::from(nameattr_file)).unwrap();

    test_flamegraph(input_file, expected_result_file, options).unwrap();
}

#[test]
#[cfg(feature = "nameattr")]
fn flamegraph_nameattr_empty_attribute() {
    let input_file = "./flamegraph/test/results/perf-cycles-instructions-01-collapsed-all.txt";
    let expected_result_file = "./tests/data/flamegraph/nameattr/nameattr.svg";
    let nameattr_file = "./tests/data/flamegraph/nameattr/nameattr_empty_attribute.txt";

    let mut options = flamegraph::Options::default();
    options.hash = true;
    options.func_frameattrs =
        flamegraph::FuncFrameAttrsMap::from_file(&PathBuf::from(nameattr_file)).unwrap();

    test_flamegraph(input_file, expected_result_file, options).unwrap();
}

#[test]
#[cfg(feature = "nameattr")]
fn flamegraph_nameattr_duplicate_attributes() {
    let input_file = "./flamegraph/test/results/perf-cycles-instructions-01-collapsed-all.txt";
    let expected_result_file = "./tests/data/flamegraph/nameattr/nameattr_duplicate_attributes.svg";
    let nameattr_file = "./tests/data/flamegraph/nameattr/nameattr_duplicate_attributes.txt";

    let mut options = flamegraph::Options::default();
    options.hash = true;
    options.func_frameattrs =
        flamegraph::FuncFrameAttrsMap::from_file(&PathBuf::from(nameattr_file)).unwrap();

    test_flamegraph(input_file, expected_result_file, options).unwrap();
}

#[test]
#[cfg(feature = "nameattr")]
fn flamegraph_nameattr_should_warn_about_duplicate_attributes() {
    testing_logger::setup();
    let nameattr_file = "./tests/data/flamegraph/nameattr/nameattr_duplicate_attributes.txt";
    let _ = flamegraph::FuncFrameAttrsMap::from_file(&PathBuf::from(nameattr_file));
    testing_logger::validate(|captured_logs| {
        let nwarnings = captured_logs
            .iter()
            .filter(|log| log.body.starts_with("duplicate attribute") && log.level == Level::Warn)
            .count();
        assert_eq!(
            nwarnings, 3,
            "invalid attribute warning logged {} times, but should be logged exactly once",
            nwarnings
        );
    });
}

#[test]
#[cfg(feature = "nameattr")]
fn flamegraph_nameattr_should_warn_about_invalid_attribute() {
    testing_logger::setup();
    let nameattr_file = "./tests/data/flamegraph/nameattr/nameattr_invalid_attribute.txt";
    let _ = flamegraph::FuncFrameAttrsMap::from_file(&PathBuf::from(nameattr_file));
    testing_logger::validate(|captured_logs| {
        let nwarnings = captured_logs
            .iter()
            .filter(|log| log.body.starts_with("invalid attribute") && log.level == Level::Warn)
            .count();
        assert_eq!(
            nwarnings, 1,
            "invalid attribute warning logged {} times, but should be logged exactly once",
            nwarnings
        );
    });
}

#[test]
fn flamegraph_should_warn_about_fractional_samples() {
    test_flamegraph_logs(
        "./tests/data/flamegraph/fractional-samples/fractional.txt",
        |captured_logs| {
            let nwarnings = captured_logs
                .iter()
                .filter(|log| {
                    log.body
                        .starts_with("The input data has fractional sample counts")
                        && log.level == Level::Warn
                })
                .count();
            assert_eq!(
                nwarnings, 1,
                "fractional samples warning logged {} times, but should be logged exactly once",
                nwarnings
            );
        },
    );
}

#[test]
fn flamegraph_should_not_warn_about_zero_fractional_samples() {
    test_flamegraph_logs(
        "./tests/data/flamegraph/fractional-samples/zero-fractionals.txt",
        |captured_logs| {
            let nwarnings = captured_logs
                .iter()
                .filter(|log| {
                    log.body
                        .starts_with("The input data has fractional sample counts")
                        && log.level == Level::Warn
                })
                .count();
            assert_eq!(
                nwarnings, 0,
                "warning about fractional samples not expected"
            );
        },
    );
}

#[test]
fn flamegraph_should_not_warn_about_fractional_sample_with_tricky_stack() {
    test_flamegraph_logs(
        "./tests/data/flamegraph/fractional-samples/tricky-stack.txt",
        |captured_logs| {
            let nwarnings = captured_logs
                .iter()
                .filter(|log| {
                    log.body
                        .starts_with("The input data has fractional sample counts")
                        && log.level == Level::Warn
                })
                .count();
            assert_eq!(
                nwarnings, 0,
                "warning about fractional samples not expected"
            );
        },
    );
}

fn load_palette_map_file(palette_file: &str) -> PaletteMap {
    let path = Path::new(palette_file);
    PaletteMap::load_from_file_or_empty(&path).unwrap()
}

#[test]
fn flamegraph_palette_map() {
    let input_file = "./flamegraph/test/results/perf-vertx-stacks-01-collapsed-all.txt";
    let expected_result_file = "./tests/data/flamegraph/palette-map/consistent-palette.svg";
    let palette_file = "./tests/data/flamegraph/palette-map/palette.map";
    let mut palette_map = load_palette_map_file(palette_file);

    let mut options = flamegraph::Options::default();
    options.palette_map = Some(&mut palette_map);
    options.hash = true;

    test_flamegraph(input_file, expected_result_file, options).unwrap();
}

#[test]
fn flamegraph_palette_map_should_warn_about_invalid_lines() {
    testing_logger::setup();
    let palette_file = "./tests/data/flamegraph/palette-map/palette_invalid.map";
    let _ = load_palette_map_file(palette_file);
    testing_logger::validate(|captured_logs| {
        let nwarnings = captured_logs
            .iter()
            .filter(|log| {
                log.body == ("Ignored 5 lines with invalid format") && log.level == Level::Warn
            })
            .count();
        assert_eq!(
            nwarnings, 1,
            "invalid palette map line warning logged {} times, but should be logged exactly once",
            nwarnings
        );
    });
}

#[test]
fn flamegraph_should_warn_about_bad_input_lines() {
    test_flamegraph_logs(
        "./tests/data/flamegraph/bad-lines/bad-lines.txt",
        |captured_logs| {
            let nwarnings = captured_logs
                .iter()
                .filter(|log| {
                    log.body.starts_with("Ignored")
                        && log.body.ends_with(" lines with invalid format")
                        && log.level == Level::Warn
                })
                .count();
            assert_eq!(
                nwarnings, 1,
                "bad lines warning logged {} times, but should be logged exactly once",
                nwarnings
            );
        },
    );
}

#[test]
fn flamegraph_should_warn_about_empty_input() {
    test_flamegraph_logs("./tests/data/flamegraph/empty/empty.txt", |captured_logs| {
        let nwarnings = captured_logs
            .iter()
            .filter(|log| log.body == "No stack counts found" && log.level == Level::Error)
            .count();
        assert_eq!(
            nwarnings, 1,
            "no stack counts error logged {} times, but should be logged exactly once",
            nwarnings
        );
    });
}

#[test]
fn flamegraph_empty_input() {
    let input_file = "./tests/data/flamegraph/empty/empty.txt";
    let expected_result_file = "./tests/data/flamegraph/empty/empty.svg";
    assert!(test_flamegraph(input_file, expected_result_file, Default::default()).is_err());
}

#[test]
fn flamegraph_unsorted_multiple_input_files() {
    let input_files = vec![
        "./tests/data/flamegraph/multiple-inputs/perf-vertx-stacks-01-collapsed-all-unsorted-1.txt"
            .into(),
        "./tests/data/flamegraph/multiple-inputs/perf-vertx-stacks-01-collapsed-all-unsorted-2.txt"
            .into(),
    ];
    let expected_result_file =
        "./tests/data/flamegraph/perf-vertx-stacks/perf-vertx-stacks-01-collapsed-all.svg";
    let mut options = flamegraph::Options::default();
    options.hash = true;
    test_flamegraph_multiple_files(input_files, expected_result_file, options).unwrap();
}

#[test]
fn flamegraph_should_prune_narrow_blocks() {
    let input_file = "./tests/data/flamegraph/narrow-blocks/narrow-blocks.txt";
    let expected_result_file = "./tests/data/flamegraph/narrow-blocks/narrow-blocks.svg";

    let mut options = flamegraph::Options::default();
    options.hash = true;

    test_flamegraph(input_file, expected_result_file, options).unwrap();
}

#[test]
fn flamegraph_inverted() {
    let input_file = "./flamegraph/test/results/perf-vertx-stacks-01-collapsed-all.txt";
    let expected_result_file = "./tests/data/flamegraph/inverted/inverted.svg";

    let mut options = flamegraph::Options::default();
    options.hash = true;
    options.title = "Icicle Graph".to_string();
    options.direction = Direction::Inverted;

    test_flamegraph(input_file, expected_result_file, options).unwrap();
}

#[test]
fn flamegraph_grey_frames() {
    let input_file = "./tests/data/flamegraph/grey-frames/grey-frames.txt";
    let expected_result_file = "./tests/data/flamegraph/grey-frames/grey-frames.svg";

    let mut options = flamegraph::Options::default();
    options.hash = true;

    test_flamegraph(input_file, expected_result_file, options).unwrap();
}

#[test]
fn flamegraph_example_perf_stacks() {
    let input_file = "./tests/data/collapse-perf/results/example-perf-stacks-collapsed.txt";
    let expected_result_file =
        "./tests/data/flamegraph/example-perf-stacks/example-perf-stacks.svg";
    let palette_file = "./tests/data/flamegraph/example-perf-stacks/palette.map";
    let mut palette_map = load_palette_map_file(palette_file);

    let mut options = flamegraph::Options::default();
    options.palette_map = Some(&mut palette_map);

    test_flamegraph(input_file, expected_result_file, options).unwrap();
}

#[test]
fn flamegraph_default_options() {
    let input_file =
        "./tests/data/flamegraph/differential/perf-cycles-instructions-01-collapsed-all-diff.txt";
    let expected_result_file = "./tests/data/flamegraph/options/default.svg";

    let options = flamegraph::Options::default();

    test_flamegraph(input_file, expected_result_file, options).unwrap();
}

#[test]
fn flamegraph_title_simple() {
    let input_file =
        "./tests/data/flamegraph/differential/perf-cycles-instructions-01-collapsed-all-diff.txt";
    let expected_result_file = "./tests/data/flamegraph/options/title_simple.svg";

    let mut options = flamegraph::Options::default();
    options.title = "Test Graph".to_owned();

    test_flamegraph(input_file, expected_result_file, options).unwrap();
}

#[test]
fn flamegraph_title_with_symbols() {
    let input_file =
        "./tests/data/flamegraph/differential/perf-cycles-instructions-01-collapsed-all-diff.txt";
    let expected_result_file = "./tests/data/flamegraph/options/title_with_symbols.svg";

    let mut options = flamegraph::Options::default();
    options.title = "Test <& ' \"".to_owned();

    test_flamegraph(input_file, expected_result_file, options).unwrap();
}

#[test]
fn flamegraph_subtitle_simple() {
    let input_file =
        "./tests/data/flamegraph/differential/perf-cycles-instructions-01-collapsed-all-diff.txt";
    let expected_result_file = "./tests/data/flamegraph/options/subtitle_simple.svg";

    let mut options = flamegraph::Options::default();
    options.subtitle = Some("Test Subtitle".to_owned());

    test_flamegraph(input_file, expected_result_file, options).unwrap();
}

#[test]
fn flamegraph_subtitle_with_symbols() {
    let input_file =
        "./tests/data/flamegraph/differential/perf-cycles-instructions-01-collapsed-all-diff.txt";
    let expected_result_file = "./tests/data/flamegraph/options/subtitle_with_symbols.svg";

    let mut options = flamegraph::Options::default();
    options.subtitle = Some("Test Subtitle <& ' \"".to_owned());

    test_flamegraph(input_file, expected_result_file, options).unwrap();
}

#[test]
fn flamegraph_notes_simple() {
    let input_file =
        "./tests/data/flamegraph/differential/perf-cycles-instructions-01-collapsed-all-diff.txt";
    let expected_result_file = "./tests/data/flamegraph/options/notes_simple.svg";

    let mut options = flamegraph::Options::default();
    options.notes = "Test Notes".to_owned();

    test_flamegraph(input_file, expected_result_file, options).unwrap();
}

#[test]
fn flamegraph_notes_with_symbols() {
    let input_file =
        "./tests/data/flamegraph/differential/perf-cycles-instructions-01-collapsed-all-diff.txt";
    let expected_result_file = "./tests/data/flamegraph/options/notes_with_symbols.svg";

    let mut options = flamegraph::Options::default();
    options.notes = "Test Notes <& ' \"".to_owned();

    test_flamegraph(input_file, expected_result_file, options).unwrap();
}

#[test]
fn flamegraph_count_name_simple() {
    let input_file =
        "./tests/data/flamegraph/differential/perf-cycles-instructions-01-collapsed-all-diff.txt";
    let expected_result_file = "./tests/data/flamegraph/options/count_name_simple.svg";

    let mut options = flamegraph::Options::default();
    options.count_name = "test-samples".to_owned();

    test_flamegraph(input_file, expected_result_file, options).unwrap();
}

#[test]
fn flamegraph_count_name_with_symbols() {
    let input_file =
        "./tests/data/flamegraph/differential/perf-cycles-instructions-01-collapsed-all-diff.txt";
    let expected_result_file = "./tests/data/flamegraph/options/count_name_with_symbols.svg";

    let mut options = flamegraph::Options::default();
    options.count_name = "test-samples <& ' \"".to_owned();

    test_flamegraph(input_file, expected_result_file, options).unwrap();
}

#[test]
fn flamegraph_name_type_simple() {
    let input_file =
        "./tests/data/flamegraph/differential/perf-cycles-instructions-01-collapsed-all-diff.txt";
    let expected_result_file = "./tests/data/flamegraph/options/name_type_simple.svg";

    let mut options = flamegraph::Options::default();
    options.name_type = "Tfunction:".to_owned();

    test_flamegraph(input_file, expected_result_file, options).unwrap();
}

#[test]
fn flamegraph_name_type_with_quote() {
    let input_file =
        "./tests/data/flamegraph/differential/perf-cycles-instructions-01-collapsed-all-diff.txt";
    let expected_result_file = "./tests/data/flamegraph/options/name_type_with_quote.svg";

    let mut options = flamegraph::Options::default();
    options.name_type = "Test: '".to_owned();

    test_flamegraph(input_file, expected_result_file, options).unwrap();
}

#[test]
fn flamegraph_name_type_with_backslash() {
    let input_file =
        "./tests/data/flamegraph/differential/perf-cycles-instructions-01-collapsed-all-diff.txt";
    let expected_result_file = "./tests/data/flamegraph/options/name_type_with_backslash.svg";

    let mut options = flamegraph::Options::default();
    options.name_type = "Test: \\".to_owned();

    test_flamegraph(input_file, expected_result_file, options).unwrap();
}

#[test]
fn flamegraph_font_type_simple() {
    let input_file =
        "./tests/data/flamegraph/differential/perf-cycles-instructions-01-collapsed-all-diff.txt";
    let expected_result_file = "./tests/data/flamegraph/options/font_type_simple.svg";

    let mut options = flamegraph::Options::default();
    options.font_type = "Andale Mono".to_owned();

    test_flamegraph(input_file, expected_result_file, options).unwrap();
}

#[test]
fn flamegraph_font_type_with_quote() {
    let input_file =
        "./tests/data/flamegraph/differential/perf-cycles-instructions-01-collapsed-all-diff.txt";
    let expected_result_file = "./tests/data/flamegraph/options/font_type_with_quote.svg";

    let mut options = flamegraph::Options::default();
    options.font_type = "Andale Mono\"".to_owned();

    test_flamegraph(input_file, expected_result_file, options).unwrap();
}

#[test]
fn flamegraph_font_type_generic_families() {
    let input_file =
        "./tests/data/flamegraph/differential/perf-cycles-instructions-01-collapsed-all-diff.txt";

    let generic_families = &["cursive", "fantasy", "monospace", "serif", "sans-serif"];
    for family in generic_families {
        let expected_result_file =
            format!("./tests/data/flamegraph/options/font_type_{}.svg", family);

        let mut options = flamegraph::Options::default();
        options.font_type = family.to_string();

        test_flamegraph(input_file, &expected_result_file, options).unwrap();
    }
}

#[test]
fn search_color_non_default() {
    let input_file =
        "./tests/data/flamegraph/differential/perf-cycles-instructions-01-collapsed-all-diff.txt";
    let expected_result_file = "./tests/data/flamegraph/options/search_color.svg";

    let mut options = flamegraph::Options::default();
    options.search_color = "#7d7d7d".parse().unwrap();

    test_flamegraph(input_file, expected_result_file, options).unwrap();
}

#[test]
fn stroke_color_non_default() {
    let input_file =
        "./tests/data/flamegraph/differential/perf-cycles-instructions-01-collapsed-all-diff.txt";
    let expected_result_file = "./tests/data/flamegraph/options/stroke_color.svg";

    let mut options = flamegraph::Options::default();
    options.stroke_color = "#7d7d7d".parse().unwrap();

    test_flamegraph(input_file, expected_result_file, options).unwrap();
}

#[test]
fn ui_color_non_default() {
    let input_file =
        "./tests/data/flamegraph/differential/perf-cycles-instructions-01-collapsed-all-diff.txt";
    let expected_result_file = "./tests/data/flamegraph/options/uicolor_color.svg";

    let mut options = flamegraph::Options::default();
    options.uicolor = rgb::RGB8 { r: 255, g: 0, b: 0 };

    test_flamegraph(input_file, expected_result_file, options).unwrap();
}

#[test]
fn flamegraph_sorted_input_file() {
    let input_file = "./flamegraph/test/results/perf-vertx-stacks-01-collapsed-all.txt";
    let expected_result_file =
        "./tests/data/flamegraph/perf-vertx-stacks/perf-vertx-stacks-01-collapsed-all.svg";

    let mut options = flamegraph::Options::default();
    options.hash = true;

    test_flamegraph(input_file, expected_result_file, options).unwrap();
}

#[test]
fn flamegraph_unsorted_input_file() {
    let input_file =
        "./tests/data/flamegraph/unsorted-input/perf-vertx-stacks-01-collapsed-all-unsorted.txt";
    let expected_result_file =
        "./tests/data/flamegraph/perf-vertx-stacks/perf-vertx-stacks-01-collapsed-all.svg";

    let mut options = flamegraph::Options::default();
    options.hash = true;

    test_flamegraph(input_file, expected_result_file, options).unwrap();
}

#[test]
fn flamegraph_no_sort_should_return_error_on_unsorted_input() {
    let input_file =
        "./tests/data/flamegraph/unsorted-input/perf-vertx-stacks-01-collapsed-all-unsorted.txt";
    let expected_result_file =
        "./tests/data/flamegraph/perf-vertx-stacks/perf-vertx-stacks-01-collapsed-all.svg";

    let mut options = flamegraph::Options::default();
    options.no_sort = true;

    assert!(test_flamegraph(input_file, expected_result_file, options).is_err());
}

#[test]
fn flamegraph_reversed_stack_ordering() {
    let input_file =
        "./tests/data/flamegraph/unsorted-input/perf-vertx-stacks-01-collapsed-all-unsorted.txt";
    let expected_result_file =
        "./tests/data/flamegraph/perf-vertx-stacks/perf-vertx-stacks-01-collapsed-all-reversed-stacks.svg";

    let mut options = flamegraph::Options::default();
    options.hash = true;
    options.reverse_stack_order = true;

    test_flamegraph(input_file, expected_result_file, options).unwrap();
}

#[test]
fn flamegraph_reversed_stack_ordering_with_fractional_samples() {
    let input_file = "./tests/data/flamegraph/fractional-samples/fractional.txt";
    let expected_result_file = "./tests/data/flamegraph/fractional-samples/fractional-reversed.svg";

    let mut options = flamegraph::Options::default();
    options.hash = true;
    options.reverse_stack_order = true;

    test_flamegraph(input_file, expected_result_file, options).unwrap();
}

#[test]
fn flamegraph_reversed_stack_ordering_with_space() {
    let input_file = "./tests/data/flamegraph/fractional-samples/with-space.txt";
    let expected_result_file = "./tests/data/flamegraph/fractional-samples/with-space-reversed.svg";

    let mut options = flamegraph::Options::default();
    options.hash = true;
    options.reverse_stack_order = true;

    test_flamegraph(input_file, expected_result_file, options).unwrap();
}

#[test]
fn flamegraph_should_warn_about_no_sort_when_reversing_stack_ordering() {
    let mut options = flamegraph::Options::default();
    options.no_sort = true;
    options.reverse_stack_order = true;

    test_flamegraph_logs_with_options(
        "./flamegraph/test/results/perf-funcab-cmd-01-collapsed-all.txt",
        |captured_logs| {
            let nwarnings = captured_logs
            .iter()
            .filter(|log| log.body == "Input lines are always sorted when `reverse_stack_order` is `true`. The `no_sort` option is being ignored." && log.level == Level::Warn)
            .count();
            assert_eq!(
                nwarnings, 1,
                "no-sort warning logged {} times, but should be logged exactly once",
                nwarnings
            );
        },
        options,
    );
}

#[test]
fn flamegraph_should_warn_about_bad_input_lines_with_reversed_stack_ordering() {
    let mut options = flamegraph::Options::default();
    options.reverse_stack_order = true;

    test_flamegraph_logs_with_options(
        "./tests/data/flamegraph/bad-lines/bad-lines.txt",
        |captured_logs| {
            let nwarnings = captured_logs
                .iter()
                .filter(|log| {
                    log.body.starts_with("Ignored")
                        && log.body.ends_with(" lines with invalid format")
                        && log.level == Level::Warn
                })
                .count();
            assert_eq!(
                nwarnings, 1,
                "bad lines warning logged {} times, but should be logged exactly once",
                nwarnings
            );
        },
        options,
    );
}

#[test]
fn flamegraph_cli() {
    let input_file = "./flamegraph/test/results/perf-vertx-stacks-01-collapsed-all.txt";
    let expected_file =
        "./tests/data/flamegraph/perf-vertx-stacks/perf-vertx-stacks-01-collapsed-all.svg";
    // Test with file passed in
    let output = Command::cargo_bin("inferno-flamegraph")
        .unwrap()
        .arg("--pretty-xml")
        .arg("--no-javascript")
        .arg("--hash")
        .arg(input_file)
        .output()
        .expect("failed to execute process");
    let expected = BufReader::new(File::open(expected_file).unwrap());
    compare_results(Cursor::new(output.stdout), expected, expected_file);

    // Test with STDIN
    let mut child = Command::cargo_bin("inferno-flamegraph")
        .unwrap()
        .arg("--pretty-xml")
        .arg("--no-javascript")
        .arg("--hash")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to spawn child process");
    let mut input = BufReader::new(File::open(input_file).unwrap());
    let stdin = child.stdin.as_mut().expect("Failed to open stdin");
    io::copy(&mut input, stdin).unwrap();
    let output = child.wait_with_output().expect("Failed to read stdout");
    let expected = BufReader::new(File::open(expected_file).unwrap());
    compare_results(Cursor::new(output.stdout), expected, expected_file);

    // Test with multiple files passed in
    let input_file_part1 =
        "./tests/data/flamegraph/multiple-inputs/perf-vertx-stacks-01-collapsed-all-unsorted-1.txt";
    let input_file_part2 =
        "./tests/data/flamegraph/multiple-inputs/perf-vertx-stacks-01-collapsed-all-unsorted-2.txt";
    let output = Command::cargo_bin("inferno-flamegraph")
        .unwrap()
        .arg("--pretty-xml")
        .arg("--no-javascript")
        .arg("--hash")
        .arg(input_file_part1)
        .arg(input_file_part2)
        .output()
        .expect("failed to execute process");
    let expected = BufReader::new(File::open(expected_file).unwrap());
    compare_results(Cursor::new(output.stdout), expected, expected_file);
}

#[test]
fn flamegraph_colors_truncate_right() {
    let input_file = "./flamegraph/test/results/perf-java-stacks-01-collapsed-all.txt";
    let expected_result_file = "./tests/data/flamegraph/options/truncate-right.svg";

    let mut opts = flamegraph::Options::default();
    opts.colors = Palette::from_str("java").unwrap();
    opts.text_truncate_direction = TextTruncateDirection::Right;
    opts.bgcolors = Some(BackgroundColor::from_str("blue").unwrap());
    opts.hash = true;

    test_flamegraph(input_file, expected_result_file, opts).unwrap();
}

#[test]
fn flamegraph_flamechart() {
    let input_file = "./tests/data/flamegraph/flamechart/flames.txt";
    let expected_result_file = "./tests/data/flamegraph/flamechart/flame.svg";

    let mut opts = flamegraph::Options::default();
    opts.title = flamegraph::defaults::CHART_TITLE.to_owned();
    opts.flame_chart = true;
    opts.hash = true;

    test_flamegraph(input_file, expected_result_file, opts).unwrap();
}

#[test]
fn flamegraph_base_symbol() {
    let input_file = "./tests/data/flamegraph/base/flames.txt";
    let expected_result_file = "./tests/data/flamegraph/base/single-base.svg";

    let mut opts = flamegraph::Options::default();
    opts.title = flamegraph::defaults::CHART_TITLE.to_owned();
    opts.base = vec!["Final".to_string()];

    test_flamegraph(input_file, expected_result_file, opts).unwrap();
}

#[test]
fn flamegraph_multiple_base_symbol() {
    let input_file = "./tests/data/flamegraph/base/flames.txt";
    let expected_result_file = "./tests/data/flamegraph/base/multi-base.svg";

    let mut opts = flamegraph::Options::default();
    opts.title = flamegraph::defaults::CHART_TITLE.to_owned();
    opts.base = vec!["Final".to_string(), "Samples".to_string()];

    test_flamegraph(input_file, expected_result_file, opts).unwrap();
}

#[test]
fn flamegraph_austin() {
    let input_file = "./tests/data/flamegraph/austin/flames.txt";
    let expected_result_file = "./tests/data/flamegraph/austin/flame.svg";
    let opts = flamegraph::Options::default();
    test_flamegraph(input_file, expected_result_file, opts).unwrap();
}
