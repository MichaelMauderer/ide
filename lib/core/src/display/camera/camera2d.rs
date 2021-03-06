//! Camera implementation which is specialized for 2D view (it computes some additional parameters,
//! like the zoom to the canvas).

use crate::prelude::*;

use crate::data::dirty;
use crate::display;
use crate::display::layout::types::*;
use crate::data::dirty::traits::*;
use crate::control::callback::CallbackRegistry1;
use crate::control::callback::CallbackHandle;
use crate::control::callback::CallbackMut1Fn;

use nalgebra::Vector2;
use nalgebra::Vector3;
use nalgebra::Matrix4;
use nalgebra::Perspective3;



// ==============
// === Screen ===
// ==============

/// Camera's frustum screen dimensions.
#[derive(Clone,Copy,Debug)]
pub struct Screen {
    /// Screen's width.
    pub width  : f32,

    /// Screen's height.
    pub height : f32,
}

impl Screen {
    /// Creates a new Screen.
    pub fn new(width:f32, height:f32) -> Self {
        Self{width,height}
    }

    /// Gets Screen's aspect ratio.
    pub fn aspect(self) -> f32 {
        self.width / self.height
    }
}



// ==================
// === Projection ===
// ==================

/// Camera's projection type.
#[derive(Clone,Copy,Debug)]
pub enum Projection {
    /// Perspective projection.
    Perspective {
        /// Perspective projection's field of view.
        fov : f32
    },

    /// Orthographic projection.
    Orthographic
}

impl Default for Projection {
    fn default() -> Self {
        Self::Perspective {fov:45.0f32.to_radians()}
    }
}



// ================
// === Clipping ===
// ================

/// Camera's frustum clipping range.
#[derive(Clone,Copy,Debug)]
pub struct Clipping {
    /// Near clipping limit.
    pub near : f32,

    /// Far clipping limit.
    pub far  : f32
}

impl Default for Clipping {
    fn default() -> Self {
        let near = 0.0;
        let far  = 1000.0;
        Self {near,far}
    }
}



// ====================
// === Camera2dData ===
// ====================

/// Function used to return the updated screen dimensions.
pub trait ScreenUpdateFn = CallbackMut1Fn<Vector2<f32>>;

/// Function used to return the updated `Camera2d`'s zoom.
pub trait ZoomUpdateFn = CallbackMut1Fn<f32>;

/// Internal `Camera2d` representation. Please see `Camera2d` for full documentation.
#[derive(Derivative)]
#[derivative(Debug)]
struct Camera2dData {
    pub transform          : display::object::Node,
    screen                 : Screen,
    zoom                   : f32,
    native_z               : f32,
    alignment              : Alignment,
    projection             : Projection,
    clipping               : Clipping,
    view_matrix            : Matrix4<f32>,
    projection_matrix      : Matrix4<f32>,
    view_projection_matrix : Matrix4<f32>,
    projection_dirty       : ProjectionDirty,
    transform_dirty        : TransformDirty2,
    zoom_update_registry   : CallbackRegistry1<f32>,
    screen_update_registry : CallbackRegistry1<Vector2<f32>>,
}

type ProjectionDirty = dirty::SharedBool<()>;
type TransformDirty2 = dirty::SharedBool<()>;

impl Camera2dData {
    pub fn new(logger:Logger, width:f32, height:f32) -> Self {
        let screen                 = Screen::new(width,height);
        let projection             = default();
        let clipping               = default();
        let alignment              = default();
        let zoom                   = 1.0;
        let native_z               = 1.0;
        let view_matrix            = Matrix4::identity();
        let projection_matrix      = Matrix4::identity();
        let view_projection_matrix = Matrix4::identity();
        let projection_dirty       = ProjectionDirty::new(logger.sub("projection_dirty"),());
        let transform_dirty        = TransformDirty2::new(logger.sub("transform_dirty"),());
        let transform_dirty_copy   = transform_dirty.clone();
        let transform              = display::object::Node::new(logger);
        let zoom_update_registry   = default();
        let screen_update_registry = default();
        transform.set_on_updated(move |_| { transform_dirty_copy.set(); });
        transform.mod_position(|p| p.z = 1.0);
        projection_dirty.set();
        let mut camera = Self {transform,screen,projection,clipping,alignment,zoom,native_z,
            view_matrix,projection_matrix,view_projection_matrix, projection_dirty,transform_dirty,
            zoom_update_registry,screen_update_registry};
        camera.set_screen(width, height);
        camera
    }

    pub fn add_zoom_update_callback<F:ZoomUpdateFn>(&mut self, f:F) -> CallbackHandle {
        self.zoom_update_registry.add(f)
    }

    pub fn add_screen_update_callback<F:ScreenUpdateFn>(&mut self, f:F) -> CallbackHandle {
        self.screen_update_registry.add(f)
    }

    pub fn recompute_view_matrix(&mut self) {
        let mut transform = self.transform.matrix();
        let half_width    = self.screen.width  / 2.0;
        let half_height   = self.screen.height / 2.0;
        let x_offset      = match self.alignment.horizontal {
            HorizontalAlignment::Left   =>  half_width,
            HorizontalAlignment::Center =>  0.0,
            HorizontalAlignment::Right  => -half_width
        };
        let y_offset = match self.alignment.vertical {
            VerticalAlignment::Bottom =>  half_height,
            VerticalAlignment::Center =>  0.0,
            VerticalAlignment::Top    => -half_height
        };

        let alignment_transform = Vector3::new(x_offset, y_offset, 0.0);
        transform.append_translation_mut(&alignment_transform);
        self.view_matrix = transform.try_inverse().unwrap()
    }

    pub fn recompute_projection_matrix(&mut self) {
        self.projection_matrix = match &self.projection {
            Projection::Perspective {fov} => {
                let aspect = self.screen.aspect();
                let near   = self.clipping.near;
                let far    = self.clipping.far;
                *Perspective3::new(aspect,*fov,near,far).as_matrix()
            }
            _ => unimplemented!()
        };
    }

    // https://github.com/rust-lang/rust-clippy/issues/4914
    #[allow(clippy::useless_let_if_seq)]
    pub fn update(&mut self) -> bool {
        self.transform.update();
        let mut changed = false;
        if self.transform_dirty.check() {
            self.recompute_view_matrix();
            self.transform_dirty.unset();
            changed = true;
        }
        if self.projection_dirty.check() {
            self.recompute_projection_matrix();
            self.projection_dirty.unset();
            changed = true;
        }
        if changed {
            self.view_projection_matrix = self.projection_matrix * self.view_matrix;
            let zoom = self.zoom;
            self.zoom_update_registry.run_all(&zoom);
        }
        changed
    }
}


// === Getters ===

impl Camera2dData {
    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    pub fn view_projection_matrix (&self) -> &Matrix4<f32> {
        &self.view_projection_matrix
    }
}


// === Setters ===

impl Camera2dData {
    pub fn projection_mut(&mut self) -> &mut Projection {
        self.projection_dirty.set();
        &mut self.projection
    }

    pub fn clipping_mut(&mut self) -> &mut Clipping {
        self.projection_dirty.set();
        &mut self.clipping
    }

    pub fn set_screen(&mut self, width:f32, height:f32) {
        self.screen.width  = width;
        self.screen.height = height;
        self.projection_dirty.set();

        match &self.projection {
            Projection::Perspective {fov} => {
                let zoom       = self.zoom;
                let alpha      = fov / 2.0;
                let native_z  = height / (2.0 * alpha.tan());
                self.native_z = native_z;
                self.mod_position_keep_zoom(|t| t.z = native_z / zoom);
            }
            _ => unimplemented!()
        };
        let dimensions = Vector2::new(width,height);
        self.screen_update_registry.run_all(&dimensions);
    }
}


// === Transform Setters ===

impl Camera2dData {
    pub fn mod_position<F:FnOnce(&mut Vector3<f32>)>(&mut self, f:F) {
        self.mod_position_keep_zoom(f);
        self.zoom = self.native_z / self.transform.position().z;
    }

    pub fn set_position(&mut self, value:Vector3<f32>) {
        self.mod_position(|p| *p = value);
    }

    pub fn set_rotation(&mut self, yaw:f32, pitch:f32, roll:f32) {
        self.transform.mod_rotation(|r| *r = Vector3::new(yaw,pitch,roll))
    }
}


// === Private Transform Setters ===

impl Camera2dData {
    fn mod_position_keep_zoom<F:FnOnce(&mut Vector3<f32>)>(&mut self, f:F) {
        self.transform.mod_position(f)
    }
}



// ================
// === Camera2d ===
// ================

/// Camera definition for 2D objects.
///
/// Although this camera implementation is defined in terms of 3D transformations under the hood,
/// it has several properties which make sense only in the context of a 2D projection:
/// - The `zoom` factor which correlates to the final image zoom. When the `zoom` parameter is set
///   to `1.0`, the units correspond 1:1 to pixels on the screen.
/// - The `native_z` value describes the z-axis distance at which the `zoom` value is `1.0`.
/// - When a new screen dimensions are provided, the camera automatically recomputes the z-axis
///   position to keep the `zoom` unchanged.
/// - The `alignment` describes where the origin is placed in the camera frustum. It is used for
///   drawing elements and scaling the view. By default, the `alignment` is set to center, which
///   defines the origin center at the center of the screen. When scaling the view, objects placed
///   in the center of the view will not move visually. If you set the alignment to bottom-left
///   corner, you will get a view which behaves like a window in window-based GUIs. When scaling
///   the window, the left-bottom corner will stay in place.
#[derive(Clone,Debug)]
pub struct Camera2d {
    rc: Rc<RefCell<Camera2dData>>
}

impl Camera2d {
    /// Creates new Camera instance.
    pub fn new<L:Into<Logger>>(logger:L, width:f32, height:f32) -> Self {
        let logger = logger.into();
        let data   = Camera2dData::new(logger,width,height);
        let rc     = Rc::new(RefCell::new(data));
        Self {rc}
    }
}


// === Modifiers ===

impl Camera2d {
    /// Sets screen dimensions.
    pub fn set_screen(&self, width:f32, height:f32) {
        self.rc.borrow_mut().set_screen(width,height)
    }

    /// Update all diry camera parameters and compute updated view-projection matrix.
    pub fn update(&self) -> bool {
        self.rc.borrow_mut().update()
    }

    /// Adds a callback to notify when `zoom` is updated.
    pub fn add_zoom_update_callback<F:ZoomUpdateFn>(&self, f:F) -> CallbackHandle {
        self.rc.borrow_mut().add_zoom_update_callback(f)
    }

    /// Adds a callback to notify when `screen` is updated.
    pub fn add_screen_update_callback<F:ScreenUpdateFn>(&self, f:F) -> CallbackHandle {
        self.rc.borrow_mut().add_screen_update_callback(f)
    }
}


// === Getters ===

impl Camera2d {
    /// Gets `Clipping`.
    pub fn clipping(&self) -> Clipping {
        self.rc.borrow().clipping
    }

    /// Gets `Screen`.
    pub fn screen(&self) -> Screen {
        self.rc.borrow().screen
    }

    /// Gets zoom.
    pub fn zoom(&self) -> f32 {
        self.rc.borrow().zoom()
    }

    /// Gets transform.
    pub fn transform(&self) -> display::object::Node {
        self.rc.borrow().transform.clone()
    }

    /// Gets `Projection` type.
    pub fn projection(&self) -> Projection {
        self.rc.borrow().projection
    }

    /// Gets Camera2d's y field of view.
    pub fn fovy(&self) -> f32 {
        (1.0 / self.projection_matrix()[(1, 1)]).atan() * 2.0
    }

    /// Gets Camera2d's half y field of view's slope.
    pub fn half_fovy_slope(&self) -> f32 {
        (self.fovy() / 2.0).tan()
    }

    /// Gets projection matrix.
    pub fn projection_matrix(&self) -> Matrix4<f32> {
        self.rc.borrow().projection_matrix
    }

    /// Gets view x projection matrix.
    pub fn view_projection_matrix(&self) -> Matrix4<f32> {
        *self.rc.borrow().view_projection_matrix()
    }
}


// === Setters ===

impl Camera2d {
    /// Modifies position.
    pub fn mod_position<F:FnOnce(&mut Vector3<f32>)>(&self, f:F) {
        self.rc.borrow_mut().mod_position(f)
    }

    /// Sets position.
    pub fn set_position(&self, value:Vector3<f32>) {
        self.rc.borrow_mut().set_position(value)
    }

    /// Sets Camera2d's rotation.
    pub fn set_rotation(&self, yaw:f32, pitch:f32, roll:f32) {
        self.rc.borrow_mut().set_rotation(yaw,pitch,roll);
    }
}

impl CloneRef for Camera2d {}