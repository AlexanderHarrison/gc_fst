use gc_fst::*;

fn main() {
    let png = lodepng::decode32_file("Logos/Training-Mode-banner-small.png").unwrap();
    assert!(png.width == 96);
    assert!(png.height == 32);
    let png_bytes: &[[u8; 4]; 32*96] = lodepng::bytemuck::cast_slice(png.buffer.as_slice()).try_into().unwrap();

    let opening_bnr = create_opening_bnr(GameInfo {
        region: GameRegion::UsOrJp,
        game_title: "Training Mode",
        developer_title: "UnclePunch and Aitch",
        full_game_title: "Training Mode v3.0 Alpha 8.0",
        full_developer_title: "UnclePunch and Aitch",
        game_description: "Improve your skills with this featureful Melee training pack!",
        banner: &RGB5A1Image::from_rgba8(&png_bytes),
    }).unwrap();

    std::fs::write("Additional ISO Files/opening.bnr", *opening_bnr).unwrap();
}
