//! End-to-end pipeline test with a real FFmpeg binary.
//!
//! Set CLIPFARM_TEST_FFMPEG to a static FFmpeg build, or have ffmpeg/ffprobe on
//! PATH. The test skips itself (loudly) when no FFmpeg is available.

use std::path::{Path, PathBuf};
use std::process::Command;

use clipfarm_core::analyze::AnalyzeOptions;
use clipfarm_core::export::ExportOptions;
use clipfarm_core::model::{ExportKind, Niche, Platform};
use clipfarm_core::pipeline;
use clipfarm_core::tools::Tools;
use clipfarm_core::AppConfig;

fn find_binary(name: &str) -> Option<PathBuf> {
    if let Ok(explicit) = std::env::var("CLIPFARM_TEST_FFMPEG") {
        let explicit = PathBuf::from(explicit);
        let directory = explicit.parent()?;
        let candidate = directory.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|path| path.is_file())
}

fn make_video(ffmpeg: &Path, output: &Path) -> anyhow::Result<()> {
    let status = Command::new(ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args([
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=1280x720:rate=30:duration=36",
        ])
        .args([
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=240:sample_rate=16000:duration=36",
        ])
        .args([
            "-filter_complex",
            "[1:a]volume='if(lt(mod(t,7),4.5),0.4,0)':eval=frame[a]",
            "-map",
            "0:v",
            "-map",
            "[a]",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-crf",
            "30",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-b:a",
            "96k",
            "-shortest",
        ])
        .arg(output)
        .status()?;
    if !status.success() {
        anyhow::bail!("ffmpeg could not generate the fixture");
    }
    Ok(())
}

#[test]
fn local_pipeline_probes_plans_and_exports() {
    let (Some(ffmpeg), Some(ffprobe)) = (find_binary("ffmpeg"), find_binary("ffprobe")) else {
        eprintln!("SKIP: ffmpeg/ffprobe not found; set CLIPFARM_TEST_FFMPEG");
        return;
    };

    let temp = std::env::temp_dir().join(format!("clipfarm-it-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp);
    std::fs::create_dir_all(&temp).expect("temp dir");
    std::env::set_var("CLIPFARM_HOME", &temp);

    let fixture = temp.join("fixture.mp4");
    make_video(&ffmpeg, &fixture).expect("fixture");

    let config = AppConfig {
        ffmpeg: Some(ffmpeg.clone()),
        ffprobe: Some(ffprobe.clone()),
        ..AppConfig::default()
    };
    let tools = Tools::discover(&config).expect("tools");

    let mut project = pipeline::create_project(
        &tools,
        fixture.to_str().unwrap(),
        Some("Integration harvest".into()),
        Niche::Podcast,
        Platform::TikTok,
        &mut |_, _| {},
    )
    .expect("project");
    assert_eq!(project.media.as_ref().unwrap().width, 1280);
    assert!((project.media.as_ref().unwrap().duration - 36.0).abs() < 1.5);

    let options = AnalyzeOptions {
        target_clips: 4,
        max_seconds: 60.0,
        min_seconds: 4.0,
    };
    let report =
        pipeline::analyze(&tools, &mut project, &options, &mut |_, _| {}).expect("analyze");
    assert!(report.clips > 0, "warnings: {:?}", report.warnings);
    assert!(!project.clips.is_empty());
    for clip in &project.clips {
        assert!(clip.duration() <= 60.0 + 1e-6);
        assert!(clip.duration() > 1.0);
        assert!(clip.start >= 0.0);
        assert!(clip.thumbnail.as_ref().is_some_and(|path| path.is_file()));
    }

    let clip = project.clips[0].clone();
    let raw_path = temp.join("raw.mp4");
    let raw = clipfarm_core::export_clip(
        &tools,
        &project,
        &clip,
        &ExportOptions::raw(&raw_path),
        &mut |_, _| {},
    )
    .expect("raw export");
    assert!(raw_path.is_file());
    assert!(raw.bytes > 0);
    let raw_info = clipfarm_core::media::probe(&tools, &raw_path).expect("probe raw");
    assert!(raw_info.duration > 0.0 && raw_info.duration <= 62.0);

    let rendered_path = temp.join("vertical.mp4");
    let mut vertical = clip.clone();
    vertical.settings.vertical = true;
    let rendered = clipfarm_core::export_clip(
        &tools,
        &project,
        &vertical,
        &ExportOptions::rendered(&rendered_path),
        &mut |_, _| {},
    )
    .expect("rendered export");
    assert!(rendered_path.is_file());
    let rendered_info = clipfarm_core::media::probe(&tools, &rendered_path).expect("probe render");
    assert_eq!(rendered_info.width, 1080);
    assert_eq!(rendered_info.height, 1920);
    assert!(rendered_info.duration <= 60.5);
    assert_eq!(rendered.kind, ExportKind::Rendered);

    let mut too_long = clip.clone();
    too_long.end = too_long.start + 61.0;
    let error = clipfarm_core::export_clip(
        &tools,
        &project,
        &too_long,
        &ExportOptions::raw(temp.join("nope.mp4")),
        &mut |_, _| {},
    )
    .expect_err("61s clip must be rejected");
    assert!(error.to_string().contains("ceiling"));

    let _ = std::fs::remove_dir_all(&temp);
}
