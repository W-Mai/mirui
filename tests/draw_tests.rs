#[cfg(test)]
mod tests {
    use mirui::draw::{DrawCommand, Renderer, SwRenderer};
    use mirui::types::{Color, Fixed, Rect};

    #[test]
    fn fill_rect_opaque() {
        let mut buf = vec![0u8; 10 * 10 * 4]; // 10x10 RGBA
        let mut r = SwRenderer::new(&mut buf, 10, 10);
        let cmd = DrawCommand::Fill {
            area: Rect {
                x: Fixed::from_int(2),
                y: Fixed::from_int(2),
                w: Fixed::from_int(3),
                h: Fixed::from_int(3),
            },
            color: Color::rgb(255, 0, 0),
            radius: Fixed::ZERO,
            opa: 255,
        };
        let clip = Rect {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            w: Fixed::from_int(10),
            h: Fixed::from_int(10),
        };
        r.draw(&cmd, &clip);

        // pixel at (2,2) should be red
        let idx = (2 * 10 + 2) * 4;
        assert_eq!(buf[idx], 255); // R
        assert_eq!(buf[idx + 1], 0); // G
        assert_eq!(buf[idx + 2], 0); // B
        assert_eq!(buf[idx + 3], 255); // A

        // pixel at (0,0) should be untouched
        assert_eq!(buf[0], 0);
        assert_eq!(buf[1], 0);
        assert_eq!(buf[2], 0);
        assert_eq!(buf[3], 0);
    }

    #[test]
    fn fill_rect_clipped() {
        let mut buf = vec![0u8; 10 * 10 * 4];
        let mut r = SwRenderer::new(&mut buf, 10, 10);
        let cmd = DrawCommand::Fill {
            area: Rect {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                w: Fixed::from_int(10),
                h: Fixed::from_int(10),
            },
            color: Color::rgb(0, 255, 0),
            radius: Fixed::ZERO,
            opa: 255,
        };
        // clip to top-left 5x5
        let clip = Rect {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            w: Fixed::from_int(5),
            h: Fixed::from_int(5),
        };
        r.draw(&cmd, &clip);

        // (3,3) inside clip → green
        let idx = (3 * 10 + 3) * 4;
        assert_eq!(buf[idx], 0);
        assert_eq!(buf[idx + 1], 255);

        // (7,7) outside clip → untouched
        let idx = (7 * 10 + 7) * 4;
        assert_eq!(buf[idx], 0);
        assert_eq!(buf[idx + 1], 0);
    }

    #[test]
    fn fill_rect_semi_transparent() {
        let mut buf = vec![0u8; 4 * 4 * 4]; // 4x4
        // pre-fill with white
        for chunk in buf.chunks_exact_mut(4) {
            chunk[0] = 255;
            chunk[1] = 255;
            chunk[2] = 255;
            chunk[3] = 255;
        }
        let mut r = SwRenderer::new(&mut buf, 4, 4);
        let cmd = DrawCommand::Fill {
            area: Rect {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                w: Fixed::from_int(4),
                h: Fixed::from_int(4),
            },
            color: Color::rgb(0, 0, 0),
            radius: Fixed::ZERO,
            opa: 128, // ~50% opacity
        };
        let clip = Rect {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            w: Fixed::from_int(4),
            h: Fixed::from_int(4),
        };
        r.draw(&cmd, &clip);

        // should be roughly half: (0*128 + 255*127)/255 ≈ 127
        let idx = 0;
        assert!(buf[idx] > 120 && buf[idx] < 135);
    }
}
