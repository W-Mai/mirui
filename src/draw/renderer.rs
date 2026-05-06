use crate::types::Rect;

use super::command::DrawCommand;

pub trait Renderer {
    fn draw(&mut self, cmd: &DrawCommand, clip: &Rect);
    fn flush(&mut self);
}
