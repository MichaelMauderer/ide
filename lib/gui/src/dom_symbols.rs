#![allow(missing_docs)]

use basegl::traits::*;

use basegl::system::web;
use basegl::system::web::NodeInserter;
use basegl::display::symbol::DomSymbol;
use web::StyleSetter;
use basegl::display::symbol::geometry::Sprite;
use basegl::display::symbol::geometry::SpriteSystem;
use basegl::display::world::*;
use basegl::display::navigation::navigator::Navigator;
use basegl::prelude::*;
use basegl::animation::animator::fixed_step::FixedStepAnimator;

use nalgebra::Vector2;
use nalgebra::Vector3;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
#[allow(dead_code)]
pub fn run_example_dom_symbols() {
    web::forward_panic_hook_to_console();
    web::set_stdout();
    init(WorldData::new(&web::get_html_element_by_id("root").unwrap()));
}

fn init(world:World) {
    let scene         = world.scene();
    let camera        = scene.camera();
    let screen        = camera.screen();
    let navigator     = Navigator::new(&scene,&camera);
    let sprite_system = SpriteSystem::new(&world);
//    let css3d_system  = Css3dSystem::new(&world);
    let dom_front_layer = scene.dom_front_layer();
    let dom_back_layer  = scene.dom_back_layer();
    world.add_child(&sprite_system);
//    world.add_child(&css3d_system);

    let mut sprites: Vec<Sprite> = default();
    let mut css3d_objects: Vec<DomSymbol> = default();
    let count = 10;
    for i in 0 .. count {
        let x      = i as f32;
        let width  = screen.width * 1.5 / count as f32;
        let height = screen.height;
        if i % 2 == 0 {
            let height     = height * 0.75;
            let size       = Vector2::new(width, height);
            let y          = screen.height / 2.0;
            let position   = Vector3::new(width / 1.5 * x + width / 2.0, y, 0.0);
            let sprite     = sprite_system.new_instance();
            sprite.size().set(size);
            sprite.mod_position(|t| *t = position);
            sprites.push(sprite);
        } else {
            let div = web::create_div();
            div.set_style_or_panic("width"  , "100%");
            div.set_style_or_panic("height" , "100%");

            let size       = Vector2::new(width, height);
            let position   = Vector3::new(width / 1.5 * x + width / 2.0, height / 2.0, 0.0);
            let object     = DomSymbol::new(&div);
//            css3d_system.add_child2(&object);
            dom_front_layer.manage(&object);
            world.add_child(&object);
            let r          = ((x + 0.0) * 16.0) as u8;
            let g          = ((x + 2.0) * 32.0) as u8;
            let b          = ((x + 4.0) * 64.0) as u8;
            let color      = iformat!("rgb({r},{g},{b})");
            div.set_style_or_panic("background-color",color);

            object.dom().append_or_panic(&div);
            object.set_size(size);
            object.mod_position(|t| *t = position);
            css3d_objects.push(object);
        }
    }
    world.display_object().update();

    let layers = vec![dom_front_layer,dom_back_layer];

    let mut i = 0;
    let animator = FixedStepAnimator::new(2.0, move |_| {
        let _keep_alive = &world;
        let _keep_alive = &navigator;
        let _keep_alive = &sprites;
        let _keep_alive = &sprite_system;

        i = (i + 1) % 2;
        for (j, object) in css3d_objects.iter_mut().enumerate() {
            layers[(i + j) % 2].manage(&object);
        }
    });
    std::mem::forget(animator);
}
