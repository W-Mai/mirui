use super::*;

#[test]
fn explicit_drivers_require_the_matching_index() {
    let names = ["software", "metal", "opengl", "opengles2"];
    assert_eq!(
        SdlGpuDriver::Auto
            .select(names.into_iter().map(Ok))
            .unwrap(),
        None
    );
    assert_eq!(
        SdlGpuDriver::OpenGl { samples: 4 }
            .select(names.into_iter().map(Ok))
            .unwrap(),
        Some(2)
    );
    assert_eq!(
        SdlGpuDriver::OpenGles2 { samples: 0 }
            .select(names.into_iter().map(Ok))
            .unwrap(),
        Some(3)
    );
    assert!(
        SdlGpuDriver::OpenGles2 { samples: 0 }
            .select(["opengl"].into_iter().map(Ok))
            .is_err()
    );
}

#[test]
fn configuration_defaults_and_msaa_contract() {
    let config = SdlGpuConfig::default();
    assert_eq!(config.driver, SdlGpuDriver::OpenGl { samples: 4 });
    assert_eq!(config.window, SdlWindowMode::Windowed { resizable: false });
    assert!(config.vsync);
    for samples in [0, 2, 4, 8] {
        assert!(SdlGpuDriver::OpenGl { samples }.validate().is_ok());
        assert!(SdlGpuDriver::OpenGles2 { samples }.validate().is_ok());
    }
    assert!(SdlGpuDriver::OpenGl { samples: 1 }.validate().is_err());
    assert!(SdlGpuDriver::OpenGles2 { samples: 1 }.validate().is_err());
}

#[test]
fn viewport_preserves_actual_dimensions_and_fractional_scale() {
    for (logical, physical) in [
        ((640, 480), (960, 720)),
        ((1920, 1080), (3840, 2160)),
        ((101, 77), (152, 116)),
    ] {
        let result = viewport(logical, physical).unwrap();
        assert_eq!(result.logical_size(), (logical.0 as u16, logical.1 as u16));
        assert_eq!(
            result.physical_size(),
            (physical.0 as u16, physical.1 as u16)
        );
    }
    assert_eq!(
        viewport((640, 480), (960, 720)).unwrap().scale(),
        Fixed::from_ratio(3, 2)
    );
    for (logical, physical) in [
        ((0, 480), (640, 480)),
        ((640, 480), (0, 0)),
        ((640, 480), (65536, 480)),
        ((640, 480), (960, 960)),
    ] {
        assert!(viewport(logical, physical).is_err());
    }
}
