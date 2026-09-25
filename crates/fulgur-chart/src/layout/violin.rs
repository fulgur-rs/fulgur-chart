//! Violin chart layout.

use crate::ir::ChartSpec;
use crate::scene::Scene;
use crate::text::TextMeasurer;

pub fn build(spec: &ChartSpec, _m: &TextMeasurer) -> Scene {
    Scene {
        width: spec.width,
        height: spec.height,
        items: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::TEST_FONT;
    use crate::frontend::chartjs;
    use crate::scene::Prim;

    #[test]
    fn violin_scene_contains_a_filled_body_path() {
        let spec = chartjs::parse(
            r#"{"type":"violin","data":{"labels":["A"],"datasets":[{"data":[[1,2,3,4,6]]}]}}"#,
            false,
        )
        .unwrap();
        let measurer = TextMeasurer::new(TEST_FONT).unwrap();
        let scene = build(&spec, &measurer);
        assert!(
            scene
                .items
                .iter()
                .any(|item| matches!(item, Prim::Path { fill: Some(_), .. }))
        );
    }
}
