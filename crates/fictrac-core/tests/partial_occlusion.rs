#[path = "support/occlusion.rs"]
mod occlusion;
use occlusion::{Case, Scene, TestResult};

#[test]
fn incomplete_atlas_survives_severe_masks_and_resumes_a_known_visible_view() -> TestResult<()> {
    let scene = Scene::new(10_000.)?;
    for (warmup, stationary) in [(40, true), (80, false)] {
        for pattern in 0..4 {
            let result = scene.audit(
                Case {
                    warmup,
                    stationary,
                    covered_percent: 70,
                    pattern,
                    acceleration: true,
                    return_to_known: true,
                },
                |_, _, _, _| Ok(()),
            )?;
            assert!(
                result.initial_coverage < 0.60,
                "fixture accidentally prelearned a complete atlas"
            );
            assert_eq!(result.rejected_mutations, 0);
            assert_eq!(
                result.masked_atlas_changed_rows, 0,
                "severe partial taught incomplete atlas: {result:?}"
            );
            assert_eq!(
                result.masked_measured, 0,
                "severe partial published motion: {result:?}"
            );
            assert!(
                result.clear_measured >= 150,
                "known visible suffix did not resume: {result:?}"
            );
            assert!(
                result.stable_clear_frame.is_some_and(|frame| frame <= 20),
                "known-view recovery stalled: {result:?}"
            );
            assert!(
                result
                    .clear_component_rms
                    .as_ref()
                    .is_some_and(|rms| rms.iter().all(|v| *v < 0.008)),
                "known suffix exceeds existing synthetic component gate: {result:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn fully_masked_startup_initializes_only_after_the_ball_is_visible() -> TestResult<()> {
    let scene = Scene::new(10_000.)?;
    let result = scene.audit(
        Case {
            warmup: 0,
            stationary: true,
            covered_percent: 100,
            pattern: 0,
            acceleration: false,
            return_to_known: false,
        },
        |_, phase, _, row| {
            if phase == "masked" {
                assert!(!row.accepted);
                assert_eq!(row.delta_camera, [0.; 3]);
            }
            Ok(())
        },
    )?;
    assert_eq!(result.rejected_mutations, 0);
    assert!(result.clear_measured >= 175);
    assert!(
        result
            .clear_component_rms
            .as_ref()
            .is_some_and(|rms| rms.iter().all(|v| *v < 0.008)),
        "clear startup suffix {result:?}"
    );
    Ok(())
}
