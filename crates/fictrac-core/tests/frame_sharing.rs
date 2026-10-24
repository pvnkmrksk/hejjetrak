use fictrac_core::GrayFrame;

#[test]
fn immutable_frame_handoffs_and_clock_edits_do_not_copy_pixels()
-> Result<(), Box<dyn std::error::Error>> {
    let original = GrayFrame::new(16, 16, vec![0.25; 256], 10.0)?;
    let retained = original.clone();
    let retimed = retained.with_timestamp(50.0)?;
    assert_eq!(original.pixels().as_ptr(), retimed.pixels().as_ptr());
    assert_eq!(original.timestamp_ms(), 10.0);
    assert_eq!(retimed.timestamp_ms(), 50.0);
    assert_eq!(original.sample(4.5, 6.25), retimed.sample(4.5, 6.25));
    for invalid in [-1.0, f64::NAN, f64::INFINITY] {
        assert!(original.clone().with_timestamp(invalid).is_err());
        assert_eq!(original.timestamp_ms(), 10.0);
        assert_eq!(original.pixels(), &[0.25; 256]);
    }
    Ok(())
}

#[test]
fn dishonest_byte_iterator_cannot_publish_an_invalid_frame() {
    struct WrongLength(std::iter::RepeatN<u8>);
    impl Iterator for WrongLength {
        type Item = u8;
        fn next(&mut self) -> Option<u8> {
            self.0.next()
        }
        fn size_hint(&self) -> (usize, Option<usize>) {
            (4, Some(4))
        }
    }
    impl ExactSizeIterator for WrongLength {}
    for actual in [0, 3, 5, usize::MAX] {
        let pixels = WrongLength(std::iter::repeat_n(128, actual));
        assert!(GrayFrame::from_u8_iter(2, 2, pixels, 0.0).is_err());
    }
}

#[test]
fn byte_inputs_match_float_constructor_exactly_and_keep_validation_bounds()
-> Result<(), Box<dyn std::error::Error>> {
    let bytes: Vec<u8> = (0..=255).collect();
    let expected = GrayFrame::new(
        16,
        16,
        bytes.iter().map(|v| f32::from(*v) / 255.0).collect(),
        1.0,
    )?;
    let actual = GrayFrame::from_u8(16, 16, &bytes, 1.0)?;
    assert_eq!(actual.pixels(), expected.pixels());
    for time in [-1.0, f64::NAN, f64::INFINITY] {
        assert!(GrayFrame::from_u8(16, 16, &bytes, time).is_err());
    }
    assert!(GrayFrame::from_u8(16, 16, &bytes[..255], 1.0).is_err());
    assert!(GrayFrame::from_u8_iter(32_000_001, 2, (0..64_000_002).map(|_| 0), 1.0).is_err());
    Ok(())
}
