use crate::render::region_pipeline::RegionDrawInstance;

#[derive(Clone, Copy)]
pub struct HighlightChipStyle {
    pub bg: [f32; 4],
    pub border: [f32; 4],
    pub radius: f32,
    pub border_thickness: f32,
}

pub fn push_centered_highlight_chip(
    chrome: &mut Vec<RegionDrawInstance>,
    center_x: f32,
    origin_y: f32,
    width: f32,
    height: f32,
    style: HighlightChipStyle,
) {
    let chip_x = center_x - width * 0.5;
    let border_thickness = style.border_thickness.max(1.0);

    chrome.push(
        RegionDrawInstance::new([chip_x, origin_y, width, height], style.border)
            .with_radius(style.radius),
    );
    chrome.push(
        RegionDrawInstance::new(
            [
                chip_x + border_thickness,
                origin_y + border_thickness,
                (width - border_thickness * 2.0).max(1.0),
                (height - border_thickness * 2.0).max(1.0),
            ],
            style.bg,
        )
        .with_radius((style.radius - border_thickness).max(2.0)),
    );
}
