use super::*;

mod argument_construction {
    use super::*;

    #[test]
    fn one_frame_is_width_times_height_times_four_bytes() {
        assert_eq!(frame_len(2, 3), 24);
        assert_eq!(frame_len(CAPTURE_W, CAPTURE_H), 1280 * 720 * 4);
        assert_eq!(frame_len(0, 720), 0);
    }

    #[test]
    fn a_file_input_loops_forever() {
        assert_eq!(
            file_input_args(Path::new("/clips/a b.webm"), 0.0),
            ["-stream_loop", "-1", "-i", "/clips/a b.webm"]
        );
    }

    #[test]
    fn the_linux_camera_input_is_v4l2_with_the_device_path() {
        assert_eq!(camera_input_args_v4l2("/dev/video0"), ["-f", "v4l2", "-i", "/dev/video0"]);
    }

    #[test]
    fn the_windows_camera_input_is_dshow_with_a_video_prefixed_device_name() {
        // No shell quoting: `Command` passes argv directly, so the
        // value is the bare `video=<name>` ffmpeg expects.
        assert_eq!(
            camera_input_args_dshow("Integrated Webcam"),
            ["-f", "dshow", "-i", "video=Integrated Webcam"]
        );
    }

    #[test]
    fn the_macos_camera_input_is_avfoundation_with_the_device_index() {
        assert_eq!(camera_input_args_avfoundation("0"), ["-f", "avfoundation", "-i", "0"]);
    }

    #[test]
    fn the_platform_dispatch_picks_this_platforms_builder() {
        let dispatched = camera_input_args("dev");
        #[cfg(target_os = "windows")]
        assert_eq!(dispatched, camera_input_args_dshow("dev"));
        #[cfg(target_os = "macos")]
        assert_eq!(dispatched, camera_input_args_avfoundation("dev"));
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        assert_eq!(dispatched, camera_input_args_v4l2("dev"));
    }

    #[test]
    fn the_output_is_vertically_flipped_raw_rgba_at_the_requested_size_and_rate_on_stdout() {
        assert_eq!(
            output_args(1280, 720, 30),
            ["-vf", "vflip", "-f", "rawvideo", "-pix_fmt", "rgba", "-s", "1280x720", "-r", "30", "pipe:1"]
        );
    }

    #[test]
    fn input_args_dispatch_on_the_input_kind() {
        assert_eq!(
            input_args_for(&VideoInput::File { path: PathBuf::from("/x.webm"), start_seconds: 0.0 }),
            file_input_args(Path::new("/x.webm"), 0.0)
        );
        assert_eq!(input_args_for(&VideoInput::Camera("cam".into())), camera_input_args("cam"));
    }

    #[test]
    fn a_positive_start_offset_adds_an_ss_flag() {
        let args = file_input_args(Path::new("/x.webm"), 12.5);
        assert_eq!(args, vec!["-stream_loop", "-1", "-ss", "12.500", "-i", "/x.webm"]);
    }

    #[test]
    fn a_zero_start_offset_omits_the_ss_flag() {
        let args = file_input_args(Path::new("/x.webm"), 0.0);
        assert_eq!(args, vec!["-stream_loop", "-1", "-i", "/x.webm"]);
    }

    #[test]
    fn only_a_file_source_is_paced() {
        assert!(VideoInput::File { path: PathBuf::from("/x.webm"), start_seconds: 0.0 }.is_paced());
        assert!(!VideoInput::Camera("cam".into()).is_paced());
    }
}

mod rate_encoding {
    use super::*;

    #[test]
    fn a_rate_round_trips_through_the_shared_atomic() {
        for rate in [0.6, 1.0, 1.25, 2.0] {
            assert!((decode_rate(encode_rate(rate)) - rate).abs() < 1e-9, "rate {rate}");
        }
    }

    #[test]
    fn an_out_of_range_rate_is_clamped_not_wrapped() {
        assert_eq!(decode_rate(encode_rate(0.0)), RATE_MIN);
        assert_eq!(decode_rate(encode_rate(-5.0)), RATE_MIN);
        assert_eq!(decode_rate(encode_rate(99.0)), RATE_MAX);
    }

    #[test]
    fn a_nan_rate_falls_back_to_normal_speed_instead_of_stalling_the_reader() {
        assert_eq!(decode_rate(encode_rate(f64::NAN)), 1.0);
        assert_eq!(decode_rate(encode_rate(f64::INFINITY)), 1.0);
    }

    #[test]
    fn no_reachable_encoded_rate_can_make_the_frame_period_infinite() {
        // Guards the `1.0 / (fps * rate)` division in `read_loop`.
        for encoded in [0u32, 1, 600, 1000, 2000, u32::MAX] {
            let period = 1.0 / (CAPTURE_FPS as f64 * decode_rate(encoded));
            assert!(period.is_finite() && period > 0.0, "encoded {encoded} gave period {period}");
        }
    }
}

mod camera_enumeration {
    use super::*;

    /// Builds a synthetic `/sys/class/video4linux`-shaped tree:
    /// `(node, name, index)`, where an `index` of `None` writes no
    /// `index` file at all.
    fn sysfs(tag: &str, nodes: &[(&str, &str, Option<&str>)]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("opendrop-video-cams-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (node, name, index) in nodes {
            std::fs::create_dir_all(dir.join(node)).unwrap();
            std::fs::write(dir.join(node).join("name"), format!("{name}\n")).unwrap();
            if let Some(index) = index {
                std::fs::write(dir.join(node).join("index"), format!("{index}\n")).unwrap();
            }
        }
        dir
    }

    #[test]
    fn a_nonexistent_sysfs_tree_yields_no_cameras() {
        assert_eq!(list_cameras_in(Path::new("/nonexistent/opendrop-video-test")), Vec::new());
    }

    #[test]
    fn cameras_are_listed_in_deterministic_device_order_with_their_node_in_the_label() {
        let dir = sysfs(
            "order",
            &[("video2", "USB2.0 Camera", Some("0")), ("video0", "Integrated Camera", Some("0"))],
        );

        let found = list_cameras_in(&dir);
        std::fs::remove_dir_all(&dir).unwrap();

        assert_eq!(
            found,
            vec![
                CameraDevice {
                    id: "/dev/video0".into(),
                    label: "Integrated Camera (/dev/video0)".into()
                },
                CameraDevice { id: "/dev/video2".into(), label: "USB2.0 Camera (/dev/video2)".into() },
            ]
        );
    }

    /// Review finding: the exact layout on the reviewer's machine (and
    /// on this one): one physical UVC webcam registering two nodes
    /// with the *same* name, only the second of which is uncapturable.
    #[test]
    fn a_uvc_webcams_metadata_node_is_not_offered_alongside_its_capture_node() {
        let name = "USB2.0 HD UVC WebCam: USB2.0 HD";
        let dir = sysfs("uvc", &[("video0", name, Some("0")), ("video1", name, Some("1"))]);

        let found = list_cameras_in(&dir);
        std::fs::remove_dir_all(&dir).unwrap();

        assert_eq!(
            found,
            vec![CameraDevice {
                id: "/dev/video0".into(),
                label: format!("{name} (/dev/video0)"),
            }],
            "only the index-0 capture node should be offered"
        );
    }

    #[test]
    fn two_physical_cameras_each_keep_their_own_primary_node() {
        let dir = sysfs(
            "two-cams",
            &[
                ("video0", "Cam A", Some("0")),
                ("video1", "Cam A", Some("1")),
                ("video2", "Cam B", Some("0")),
                ("video3", "Cam B", Some("1")),
            ],
        );

        let found = list_cameras_in(&dir);
        std::fs::remove_dir_all(&dir).unwrap();

        assert_eq!(found.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["/dev/video0", "/dev/video2"]);
    }

    /// Two identical webcams of the same model share a `name`; the
    /// device node in the label is what keeps them apart.
    #[test]
    fn identical_models_stay_distinguishable_through_their_device_node() {
        let dir = sysfs("twins", &[("video0", "C920", Some("0")), ("video2", "C920", Some("0"))]);

        let found = list_cameras_in(&dir);
        std::fs::remove_dir_all(&dir).unwrap();

        let labels: Vec<&str> = found.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(labels, ["C920 (/dev/video0)", "C920 (/dev/video2)"]);
        assert_ne!(labels[0], labels[1], "the whole point: no unlabeled duplicates");
    }

    #[test]
    fn a_node_with_no_index_file_is_kept_rather_than_hidden() {
        // "Can't tell" must not hide a working camera: an older
        // kernel, or a driver that doesn't publish `index`.
        let dir = sysfs("no-index", &[("video0", "Odd Camera", None)]);

        let found = list_cameras_in(&dir);
        std::fs::remove_dir_all(&dir).unwrap();

        assert_eq!(found, vec![CameraDevice { id: "/dev/video0".into(), label: "Odd Camera (/dev/video0)".into() }]);
    }

    #[test]
    fn an_unparseable_index_is_treated_as_primary() {
        let dir = std::env::temp_dir().join(format!("opendrop-video-cams-badidx-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("video0")).unwrap();
        std::fs::write(dir.join("video0").join("index"), "not a number\n").unwrap();
        assert!(!is_primary_capture_node(&dir.join("video0")), "a non-'0' index is not primary");
        std::fs::write(dir.join("video0").join("index"), "0").unwrap();
        assert!(is_primary_capture_node(&dir.join("video0")), "'0' with no trailing newline is primary");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn our_own_v4l2loopback_sink_is_never_offered_as_a_camera() {
        let dir = sysfs("loopback", &[("video0", "OpenDrop Video", Some("0")), ("video1", "Real Camera", Some("0"))]);

        let found = list_cameras_in(&dir);
        std::fs::remove_dir_all(&dir).unwrap();

        assert_eq!(
            found,
            vec![CameraDevice { id: "/dev/video1".into(), label: "Real Camera (/dev/video1)".into() }]
        );
    }

    /// Real-environment check: whatever this machine has, no two
    /// entries may share a label: that is exactly the state the
    /// review flagged.
    #[test]
    fn no_two_cameras_on_this_machine_share_a_label() {
        let cameras = list_cameras();
        let mut labels: Vec<&str> = cameras.iter().map(|c| c.label.as_str()).collect();
        let total = labels.len();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), total, "duplicate camera labels in {cameras:?}");
    }
}

mod snapshot_and_handle {
    use super::*;

    #[test]
    fn a_fresh_snapshot_is_idle_with_no_error() {
        let s = VideoCaptureSnapshot::default();
        assert!(!s.running);
        assert_eq!(s.last_error, None);
    }

    #[test]
    fn a_freshly_spawned_thread_is_idle_and_publishes_no_frame() {
        let handle = spawn();
        assert!(!handle.latest().running);
        assert!(handle.latest_frame().is_none());
    }

    #[test]
    fn a_start_on_a_nonexistent_file_reports_an_error_without_panicking() {
        let handle = spawn();
        let _ = handle
            .control_tx
            .send(VideoCaptureControl::Start(VideoInput::File {
                path: PathBuf::from("/nonexistent/opendrop-no-such.webm"),
                start_seconds: 0.0,
            }));
        // ffmpeg is spawned successfully but exits immediately (it
        // can't open the file); either the spawn failed outright (no
        // ffmpeg on PATH) or the liveness poll notices the exit. Both
        // land on `running == false` with an error. Poll briefly
        // rather than sleeping a fixed amount.
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            let s = handle.latest();
            if !s.running && s.last_error.is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("a bad file should have surfaced an error, got {:?}", handle.latest());
    }
}

/// Real-subprocess tests. Every one of them degrades to a silent pass
/// when `ffmpeg` isn't on PATH (same "structural check only" convention
/// as `v4l2loopback`'s own spawn test), but on a machine that has it
/// (every machine this app can actually run the v4l2 output on), they
/// exercise the parts no amount of `cargo build` can: that the argument
/// set really produces frames of exactly the size we claim, and that
/// `Drop` really leaves no process behind.
mod real_ffmpeg {
    use super::*;

    /// A synthetic 64x48 source, so a frame is 12 KB rather than 3.5 MB
    /// and one arrives in milliseconds.
    fn testsrc() -> Vec<String> {
        vec!["-f".into(), "lavfi".into(), "-i".into(), "testsrc=size=64x48:rate=30".into()]
    }

    #[test]
    fn spawning_does_not_panic_whether_or_not_the_input_is_valid() {
        let bogus = file_input_args(Path::new("/nonexistent/opendrop-no-such.webm"), 0.0);
        if let Ok((mut src, _stdout)) = VideoSource::spawn(&bogus, 64, 48, 30) {
            let _ = src.is_alive();
        }
        // else: ffmpeg not on PATH here, also not a panic.
    }

    #[test]
    fn a_real_pipe_yields_exactly_one_frames_worth_of_rgba_bytes() {
        let Ok((src, mut stdout)) = VideoSource::spawn(&testsrc(), 64, 48, 30) else {
            return; // no ffmpeg on PATH
        };
        let mut buf = vec![0u8; frame_len(64, 48)];
        let read = stdout.read_exact(&mut buf);
        drop(src);
        assert!(read.is_ok(), "reading one 64x48 RGBA frame failed: {read:?}");
        // `testsrc` is a color pattern with an opaque alpha channel: a
        // pix_fmt/size mismatch would show up as an all-zero buffer.
        assert!(buf.iter().any(|&b| b != 0), "the frame was entirely zero: wrong pix_fmt or size?");
        assert!(buf.as_chunks::<4>().0.iter().all(|px| px[3] == 255), "rgba alpha should be opaque");
    }

    /// Proves the `-vf vflip` in [`output_args`] really lands row 0 of
    /// the source at the BOTTOM of the emitted buffer: the one thing
    /// standing between a decoded clip and an upside-down video layer,
    /// and something no argument-shape assertion can show.
    ///
    /// Hermetic: a hand-built 1x2 RGBA source (top row red, bottom row
    /// blue) piped straight back out at the same size, so nothing here
    /// depends on a codec, a scaler, or a test pattern's own layout.
    #[test]
    fn the_filter_chain_flips_the_image_into_gls_bottom_left_origin() {
        let dir = std::env::temp_dir().join(format!("opendrop-video-flip-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src_path = dir.join("two-rows.rgba");
        // Row 0 (TOP) red, row 1 (BOTTOM) blue.
        std::fs::write(&src_path, [255u8, 0, 0, 255, 0, 0, 255, 255]).unwrap();

        let input = vec![
            "-f".into(),
            "rawvideo".into(),
            "-pix_fmt".into(),
            "rgba".into(),
            "-s".into(),
            "1x2".into(),
            "-i".into(),
            src_path.to_string_lossy().into_owned(),
        ];
        let Ok((source, mut stdout)) = VideoSource::spawn(&input, 1, 2, 30) else {
            let _ = std::fs::remove_dir_all(&dir);
            return; // no ffmpeg on PATH
        };
        let mut buf = [0u8; 8];
        let read = stdout.read_exact(&mut buf);
        drop(source);
        let _ = std::fs::remove_dir_all(&dir);

        assert!(read.is_ok(), "reading the 1x2 frame back failed: {read:?}");
        // After the flip, the first bytes out are the source's BOTTOM
        // row (blue), which is exactly what GL wants at v = 0.
        assert_eq!(&buf[0..4], &[0, 0, 255, 255], "first emitted row should be the source's bottom row");
        assert_eq!(&buf[4..8], &[255, 0, 0, 255], "last emitted row should be the source's top row");
    }

    /// End-to-end through the real handle: a real clip file on disk
    /// reaches [`VideoCaptureHandle::latest_frame`] as full-size RGBA
    /// frames with a strictly increasing `seq`, and `Stop` takes the
    /// snapshot back to idle. This is the one test that exercises the
    /// whole two-thread lifecycle rather than a piece of it.
    #[test]
    fn a_real_clip_file_reaches_the_handle_as_full_size_rgba_frames() {
        let dir = std::env::temp_dir().join(format!("opendrop-video-e2e-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let clip = dir.join("clip.avi");
        // mpeg4/avi rather than the app's usual webm: both muxer and
        // encoder are built into every ffmpeg, and encoding half a
        // second of 64x48 takes milliseconds.
        let encoded = Command::new("ffmpeg")
            .args(["-y", "-f", "lavfi", "-i", "testsrc=size=64x48:rate=30:duration=0.5", "-c:v", "mpeg4"])
            .arg(&clip)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if !matches!(encoded, Ok(s) if s.success()) {
            let _ = std::fs::remove_dir_all(&dir);
            return; // no ffmpeg on PATH, or it can't encode here
        }

        let handle = spawn();
        let _ = handle.control_tx.send(VideoCaptureControl::Start(VideoInput::File { path: clip, start_seconds: 0.0 }));

        let deadline = Instant::now() + Duration::from_secs(10);
        let mut seen: Vec<u64> = Vec::new();
        while Instant::now() < deadline && seen.len() < 3 {
            if let Some(frame) = handle.latest_frame() {
                if seen.last() != Some(&frame.seq) {
                    assert_eq!((frame.width, frame.height), (CAPTURE_W, CAPTURE_H));
                    assert_eq!(frame.data.len(), frame_len(CAPTURE_W, CAPTURE_H));
                    seen.push(frame.seq);
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let running_while_started = handle.latest().running;

        let _ = handle.control_tx.send(VideoCaptureControl::Stop);
        let stop_deadline = Instant::now() + Duration::from_secs(5);
        while handle.latest().running && Instant::now() < stop_deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        let running_after_stop = handle.latest().running;
        let _ = std::fs::remove_dir_all(&dir);

        assert!(running_while_started, "the source should report running: {:?}", handle.latest());
        assert!(seen.len() >= 3, "expected at least 3 distinct frames, got {seen:?}");
        assert!(seen.windows(2).all(|w| w[1] > w[0]), "seq must strictly increase: {seen:?}");
        assert!(!running_after_stop, "Stop should take the source back to idle");
    }

    /// Review finding: `VideoFrame::seq` documents a counter that is
    /// never reset by a `Start`. It used to be handed to each reader
    /// thread by value, so every restart really did reset it, which is
    /// a false promise a future consumer comparing with `>` would have
    /// trusted.
    ///
    /// Deterministic on purpose: it drives two `read_loop`s directly
    /// over two finite sources (what a clip cut produces) sharing the
    /// counter the run loop owns, instead of racing a live restart.
    /// A timing-based version of this test passes even against the old
    /// by-value counter, because a reset sequence climbs back past any
    /// fixed floor within a few hundred milliseconds; comparing the two
    /// runs' own last values cannot be fooled that way.
    #[test]
    fn one_shared_counter_spans_every_reader_thread() {
        // Finite duration, so each `read_loop` returns at EOF instead of
        // needing to be killed; unpaced, so it runs at full speed.
        let source_args = vec![
            "-f".to_string(),
            "lavfi".to_string(),
            "-i".to_string(),
            "testsrc=size=64x48:rate=30:duration=0.2".to_string(),
        ];
        let seq = Arc::new(AtomicU64::new(0));
        let frame: Arc<ArcSwapOption<VideoFrame>> = Arc::new(ArcSwapOption::empty());
        let rate = Arc::new(AtomicU32::new(encode_rate(1.0)));

        let mut last_seq_per_run = Vec::new();
        for run in 0..2 {
            let Ok((source, stdout)) = VideoSource::spawn(&source_args, CAPTURE_W, CAPTURE_H, CAPTURE_FPS)
            else {
                return; // no ffmpeg on PATH
            };
            read_loop(stdout, frame.clone(), rate.clone(), false, seq.clone());
            drop(source);
            let published = frame.load_full().unwrap_or_else(|| panic!("run {run} published no frame"));
            last_seq_per_run.push(published.seq);
        }

        assert!(
            last_seq_per_run[0] > 0,
            "the first run should have published frames, got {last_seq_per_run:?}"
        );
        assert!(
            last_seq_per_run[1] > last_seq_per_run[0],
            "seq restarted across sources ({last_seq_per_run:?}); it must keep climbing"
        );
    }

    /// The zombie check the module doc comment promises: after `Drop`,
    /// the child must be both killed and reaped.
    #[test]
    fn dropping_a_source_kills_and_reaps_its_ffmpeg_child() {
        let Ok((mut src, _stdout)) = VideoSource::spawn(&testsrc(), 64, 48, 30) else {
            return; // no ffmpeg on PATH
        };
        let pid = src.child.id();
        assert!(src.is_alive(), "ffmpeg should still be running before the drop");
        drop(src);

        #[cfg(target_os = "linux")]
        {
            // Either the pid is gone entirely, or (pid reuse) it is some
            // other process: what must never be true is that it is
            // still there in state Z (zombie), i.e. exited but unreaped.
            // `/proc/<pid>/stat` is `pid (comm) state ...`, and `comm`
            // may itself contain spaces and parens, so the state field
            // is read after the LAST ')'.
            if let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) {
                let after_comm = stat.rsplit_once(')').map(|(_, rest)| rest.trim_start()).unwrap_or("");
                assert!(!after_comm.starts_with('Z'), "ffmpeg child {pid} was left as a zombie: {stat}");
            }
        }
        #[cfg(not(target_os = "linux"))]
        let _ = pid;
    }
}
