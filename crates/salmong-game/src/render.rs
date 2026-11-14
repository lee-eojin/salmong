use crate::bus::BusTrack;
use crate::input::{PendingCommands, SimRes, read_pointer};
use bevy::asset::{AssetPath, RenderAssetUsages, embedded_asset, embedded_path};
use bevy::camera::ScalingMode;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;
use bevy::sprite_render::{AlphaMode2d, Material2d, Material2dPlugin};
use salmong_core::ids::{BusId, RouteId, StationId, Tick};
use salmong_core::network::NetworkSnapshot;
use salmong_core::scenario::{Scenario, Shape, StationSpec};
use salmong_core::sim::{Command, SimView};
use std::f32::consts::TAU;

pub const QUEUE_CAP: usize = 16;
pub const SLOT_COLS: usize = 4;
pub const STATION_HALF: f32 = 0.8;
pub const PASSENGER_HALF: f32 = 0.2;
pub const BADGE_HALF: f32 = 0.2;
pub const RIDER_HALF: f32 = 0.1;
pub const ROUTE_HALF: f32 = 0.15;
pub const BUS_HALF: Vec2 = Vec2::new(0.6, 0.35);
pub const RIDERS_SHOWN: u32 = 6;

const fn hex(r: u8, g: u8, b: u8) -> Srgba {
    Srgba::rgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
}

pub const STATION_COLOUR: Srgba = hex(0xFF, 0xFF, 0xFF);
pub const OVERFLOW_COLOUR: Srgba = hex(0xFF, 0xD6, 0x0A);
pub const PASSENGER_COLOUR: Srgba = hex(0x11, 0x11, 0x11);
pub const BADGE_COLOUR: Srgba = hex(0xD0, 0x00, 0x00);
pub const BUS_COLOUR: Srgba = hex(0x14, 0x21, 0x3D);
pub const RIDER_COLOUR: Srgba = hex(0xFF, 0xFF, 0xFF);
pub const SIGN_COLOUR: Srgba = hex(0x14, 0x21, 0x3D);
pub const PANEL_COLOUR: Srgba = hex(0x14, 0x21, 0x3D);
pub const DIGIT_COLOUR: Srgba = hex(0xFF, 0xFF, 0xFF);
pub const BEST_COLOUR: Srgba = hex(0xFF, 0xD6, 0x0A);
pub const GHOST_COLOUR: Srgba = hex(0x11, 0x11, 0x11);
pub const GHOST_HALF: f32 = 0.07;
pub const BAR_HALF: Vec2 = Vec2::new(0.3, 0.9);
pub const TRIANGLE_HALF: f32 = 0.8;
pub const PANEL_HALF: Vec2 = Vec2::new(12.0, 7.0);
pub const RUN_ICON_HALF: f32 = 1.0;
pub const BEST_ICON_HALF: f32 = 1.2;
pub const SEGMENT_LONG: f32 = 0.55;
pub const SEGMENT_SHORT: f32 = 0.15;
pub const DIGIT_STEP: f32 = 2.2;
pub const DIGIT_MAX: u64 = 9_999_999;
pub const ROUTE_COLOURS: [Srgba; 8] = [
    hex(0x1B, 0x9A, 0xAA),
    hex(0x6A, 0x4C, 0x93),
    hex(0x2A, 0x9D, 0x8F),
    hex(0xF4, 0xD3, 0x5E),
    hex(0x3A, 0x86, 0xFF),
    hex(0x8D, 0x99, 0xAE),
    hex(0x5F, 0x0F, 0x40),
    hex(0x38, 0x66, 0x41),
];

pub fn station_colour(overflow: u32) -> Srgba {
    if overflow > 0 {
        OVERFLOW_COLOUR
    } else {
        STATION_COLOUR
    }
}

pub fn route_colour(route: RouteId) -> Srgba {
    ROUTE_COLOURS[usize::from(route.0) % ROUTE_COLOURS.len()]
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StationInst {
    pub id: StationId,
    pub pos: Vec2,
    pub shape: Shape,
    pub overflow: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PassengerInst {
    pub station: StationId,
    pub slot: u8,
    pub pos: Vec2,
    pub want: Shape,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BadgeInst {
    pub station: StationId,
    pub pos: Vec2,
    pub extra: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BusInst {
    pub id: BusId,
    pub pos: Vec2,
    pub riders: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RouteInst {
    pub id: RouteId,
    pub points: Vec<Vec2>,
}

#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct Scene {
    pub tick: Option<Tick>,
    pub stations: Vec<StationInst>,
    pub passengers: Vec<PassengerInst>,
    pub badges: Vec<BadgeInst>,
    pub riders: Vec<(BusId, u32)>,
}

#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct RouteDraw {
    pub rev: Option<u64>,
    pub routes: Vec<RouteInst>,
}

#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct BusDraw {
    pub buses: Vec<BusInst>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OverInst {
    pub score: u64,
    pub best: Option<u64>,
}

#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct HudDraw {
    pub grid: Vec2,
    pub paused: bool,
    pub fast: bool,
    pub over: Option<OverInst>,
}

#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct PendingDraw {
    pub lines: Vec<Vec<Vec2>>,
}

fn grid_pos(spec: &StationSpec) -> Vec2 {
    Vec2::new(f32::from(spec.x), f32::from(spec.y))
}

pub fn slot_pos(station: Vec2, slot: usize) -> Vec2 {
    station
        + Vec2::new(
            1.1 + 0.45 * (slot % SLOT_COLS) as f32,
            -0.7 + 0.45 * (slot / SLOT_COLS) as f32,
        )
}

pub fn scene(view: &SimView, stations: &[StationSpec]) -> Scene {
    let mut out = Scene {
        tick: Some(view.tick),
        ..default()
    };
    for (i, queue) in view.queues.iter().enumerate() {
        let id = StationId(i as u16);
        let spec = &stations[i];
        let pos = grid_pos(spec);
        out.stations.push(StationInst {
            id,
            pos,
            shape: spec.shape,
            overflow: view.overflow[i],
        });
        for (slot, p) in queue.iter().take(QUEUE_CAP).enumerate() {
            out.passengers.push(PassengerInst {
                station: id,
                slot: slot as u8,
                pos: slot_pos(pos, slot),
                want: p.want,
            });
        }
        if queue.len() > QUEUE_CAP {
            out.badges.push(BadgeInst {
                station: id,
                pos: slot_pos(pos, QUEUE_CAP),
                extra: (queue.len() - QUEUE_CAP) as u32,
            });
        }
    }
    out.riders = view
        .buses
        .iter()
        .map(|b| (b.id, b.riders.len() as u32))
        .collect();
    out
}

pub fn routes(net: &NetworkSnapshot, stations: &[StationSpec]) -> Vec<RouteInst> {
    (0..net.route_slots())
        .filter_map(|slot| {
            let id = RouteId(slot as u8);
            net.stops(id).map(|stops| RouteInst {
                id,
                points: stops
                    .iter()
                    .map(|s| grid_pos(&stations[usize::from(s.0)]))
                    .collect(),
            })
        })
        .collect()
}

pub fn buses(at: &[(BusId, Vec2)], riders: &[(BusId, u32)]) -> Vec<BusInst> {
    at.iter()
        .map(|&(id, pos)| BusInst {
            id,
            pos,
            riders: riders
                .iter()
                .find(|(rid, _)| *rid == id)
                .map_or(0, |&(_, n)| n),
        })
        .collect()
}

pub fn to_world(grid: Vec2) -> Vec2 {
    Vec2::new(grid.x, -grid.y)
}

pub fn camera_fit(scenario: &Scenario) -> (Vec2, Vec2) {
    let size = Vec2::new(f32::from(scenario.w), f32::from(scenario.h));
    (to_world(size / 2.0), size)
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Geometry {
    pub positions: Vec<[f32; 3]>,
    pub colors: Vec<[f32; 4]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

pub const SOLID: u32 = 2;

const CORNERS: [Vec2; 4] = [
    Vec2::new(-1.0, -1.0),
    Vec2::new(1.0, -1.0),
    Vec2::new(1.0, 1.0),
    Vec2::new(-1.0, 1.0),
];

pub fn shape_code(shape: Shape) -> u32 {
    match shape {
        Shape::Circle => 0,
        Shape::Triangle => 1,
        Shape::Square => 2,
        Shape::Star => 3,
    }
}

pub fn shape_distance(code: u32, local: Vec2) -> f32 {
    match code {
        0 => local.length() - 1.0,
        1 => (-local.y).max(0.866_025_4 * local.x.abs() + 0.5 * local.y) - 0.5,
        3 => {
            let len = local.length();
            if len < 1e-6 {
                return -1.0;
            }
            let sector = TAU / 5.0;
            let a = local.x.atan2(local.y);
            let folded = ((a + sector / 2.0).rem_euclid(sector) - sector / 2.0).abs();
            let q = Vec2::new(folded.sin(), folded.cos()) * len;
            let tip = Vec2::new(0.0, 1.0);
            let inner = Vec2::new((sector / 2.0).sin(), (sector / 2.0).cos()) * 0.45;
            let edge = inner - tip;
            let outward = Vec2::new(-edge.y, edge.x).normalize();
            (q - tip).dot(outward)
        }
        _ => -1.0,
    }
}

fn barycentric(p: Vec2, a: Vec2, b: Vec2, c: Vec2) -> Option<Vec3> {
    let area = (b - a).perp_dot(c - a);
    if area.abs() < 1e-12 {
        return None;
    }
    let wa = (b - p).perp_dot(c - p) / area;
    let wb = (c - p).perp_dot(a - p) / area;
    let wc = 1.0 - wa - wb;
    let eps = -1e-6;
    (wa >= eps && wb >= eps && wc >= eps).then_some(Vec3::new(wa, wb, wc))
}

pub fn hit(g: &Geometry, world: Vec2) -> Option<[f32; 4]> {
    let mut out = None;
    for t in g.indices.chunks(3) {
        let i = [t[0] as usize, t[1] as usize, t[2] as usize];
        let v = |k: usize| Vec2::new(g.positions[k][0], g.positions[k][1]);
        let Some(w) = barycentric(world, v(i[0]), v(i[1]), v(i[2])) else {
            continue;
        };
        let uv = Vec2::from(g.uvs[i[0]]) * w.x
            + Vec2::from(g.uvs[i[1]]) * w.y
            + Vec2::from(g.uvs[i[2]]) * w.z;
        let code = (uv.x / 4.0).round();
        let local = Vec2::new(uv.x - 4.0 * code, uv.y);
        if shape_distance(code as u32, local) <= 0.0 {
            out = Some(g.colors[i[0]]);
        }
    }
    out
}

impl Geometry {
    fn append(&mut self, other: Geometry) {
        let base = self.positions.len() as u32;
        self.positions.extend(other.positions);
        self.colors.extend(other.colors);
        self.uvs.extend(other.uvs);
        self.indices
            .extend(other.indices.into_iter().map(|i| base + i));
    }

    fn instance(&mut self, corners: [Vec2; 4], code: u32, colour: Srgba) {
        let c = LinearRgba::from(colour).to_f32_array();
        let base = self.positions.len() as u32;
        for (p, l) in corners.iter().zip(CORNERS) {
            self.positions.push([p.x, p.y, 0.0]);
            self.colors.push(c);
            self.uvs.push([l.x + 4.0 * code as f32, l.y]);
        }
        self.indices
            .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    fn quad(&mut self, centre: Vec2, half: Vec2, code: u32, colour: Srgba) {
        self.instance(CORNERS.map(|l| centre + l * half), code, colour);
    }

    fn shape(&mut self, shape: Shape, centre: Vec2, r: f32, colour: Srgba) {
        self.quad(centre, Vec2::splat(r), shape_code(shape), colour);
    }

    fn segment(&mut self, a: Vec2, b: Vec2, half: f32, colour: Srgba) {
        let n = (b - a).normalize_or_zero().perp() * half;
        self.instance([a + n, a - n, b - n, b + n], SOLID, colour);
    }
}

pub fn route_geometry(draw: &RouteDraw) -> Geometry {
    let mut g = Geometry::default();
    for r in &draw.routes {
        for pair in r.points.windows(2) {
            g.segment(
                to_world(pair[0]),
                to_world(pair[1]),
                ROUTE_HALF,
                route_colour(r.id),
            );
        }
    }
    g
}

pub fn station_geometry(scene: &Scene) -> Geometry {
    let mut g = Geometry::default();
    for s in &scene.stations {
        g.shape(
            s.shape,
            to_world(s.pos),
            STATION_HALF,
            station_colour(s.overflow),
        );
    }
    g
}

pub fn passenger_geometry(scene: &Scene) -> Geometry {
    let mut g = Geometry::default();
    for p in &scene.passengers {
        g.shape(p.want, to_world(p.pos), PASSENGER_HALF, PASSENGER_COLOUR);
    }
    for b in &scene.badges {
        g.quad(
            to_world(b.pos),
            Vec2::splat(BADGE_HALF),
            SOLID,
            BADGE_COLOUR,
        );
    }
    g
}

pub fn rider_offset(k: u32) -> Vec2 {
    Vec2::new(-0.5 + 0.2 * k as f32, 0.15)
}

pub fn bus_geometry(draw: &BusDraw) -> Geometry {
    let mut g = Geometry::default();
    for b in &draw.buses {
        let c = to_world(b.pos);
        g.quad(c, BUS_HALF, SOLID, BUS_COLOUR);
        for k in 0..b.riders.min(RIDERS_SHOWN) {
            g.quad(
                c + rider_offset(k),
                Vec2::splat(RIDER_HALF),
                SOLID,
                RIDER_COLOUR,
            );
        }
    }
    g
}

pub fn digit_segments(d: u8) -> [bool; 7] {
    let lit: &[u8] = match d {
        0 => b"abcdef",
        1 => b"bc",
        2 => b"abdeg",
        3 => b"abcdg",
        4 => b"bcfg",
        5 => b"acdfg",
        6 => b"acdefg",
        7 => b"abc",
        8 => b"abcdefg",
        9 => b"abcdfg",
        _ => b"",
    };
    let mut out = [false; 7];
    for &c in lit {
        out[usize::from(c - b'a')] = true;
    }
    out
}

pub fn segment_centres(centre: Vec2) -> [(Vec2, Vec2); 7] {
    let h = Vec2::new(SEGMENT_LONG, SEGMENT_SHORT);
    let v = Vec2::new(SEGMENT_SHORT, SEGMENT_LONG);
    [
        (centre + Vec2::new(0.0, -1.4), h),
        (centre + Vec2::new(0.7, -0.7), v),
        (centre + Vec2::new(0.7, 0.7), v),
        (centre + Vec2::new(0.0, 1.4), h),
        (centre + Vec2::new(-0.7, 0.7), v),
        (centre + Vec2::new(-0.7, -0.7), v),
        (centre, h),
    ]
}

pub fn digits(value: u64) -> Vec<u8> {
    value
        .min(DIGIT_MAX)
        .to_string()
        .bytes()
        .map(|b| b - b'0')
        .collect()
}

pub fn sign_anchor(grid: Vec2) -> Vec2 {
    Vec2::new(grid.x - 3.0, 2.0)
}

pub fn bar_centres(grid: Vec2) -> [Vec2; 2] {
    let a = sign_anchor(grid);
    [a - Vec2::new(0.5, 0.0), a + Vec2::new(0.5, 0.0)]
}

pub fn triangle_centres(grid: Vec2) -> [Vec2; 2] {
    let a = sign_anchor(grid);
    [a - Vec2::new(4.0, 0.0), a - Vec2::new(2.6, 0.0)]
}

pub fn run_row(grid: Vec2) -> (Vec2, Vec2) {
    let c = grid / 2.0;
    (c + Vec2::new(-8.0, -3.0), c + Vec2::new(-4.0, -3.0))
}

pub fn best_row(grid: Vec2) -> (Vec2, Vec2) {
    let c = grid / 2.0;
    (c + Vec2::new(-8.0, 3.0), c + Vec2::new(-4.0, 3.0))
}

fn number(g: &mut Geometry, first: Vec2, value: u64) {
    for (k, d) in digits(value).into_iter().enumerate() {
        let centre = first + Vec2::new(DIGIT_STEP * k as f32, 0.0);
        for (lit, (c, half)) in digit_segments(d).into_iter().zip(segment_centres(centre)) {
            if lit {
                g.quad(to_world(c), half, SOLID, DIGIT_COLOUR);
            }
        }
    }
}

pub fn hud_geometry(hud: &HudDraw) -> Geometry {
    let mut g = Geometry::default();
    if hud.paused {
        for c in bar_centres(hud.grid) {
            g.quad(to_world(c), BAR_HALF, SOLID, SIGN_COLOUR);
        }
    }
    if hud.fast {
        for c in triangle_centres(hud.grid) {
            let w = to_world(c);
            g.instance(
                CORNERS.map(|l| w + TRIANGLE_HALF * Vec2::new(l.y, -l.x)),
                shape_code(Shape::Triangle),
                SIGN_COLOUR,
            );
        }
    }
    if let Some(over) = hud.over {
        g.quad(to_world(hud.grid / 2.0), PANEL_HALF, SOLID, PANEL_COLOUR);
        let (icon, first) = run_row(hud.grid);
        g.shape(Shape::Circle, to_world(icon), RUN_ICON_HALF, DIGIT_COLOUR);
        number(&mut g, first, over.score);
        let (icon, first) = best_row(hud.grid);
        g.shape(Shape::Star, to_world(icon), BEST_ICON_HALF, BEST_COLOUR);
        if let Some(best) = over.best {
            number(&mut g, first, best);
        }
    }
    g
}

pub fn pending_lines(
    pending: &[Command],
    routes: &[RouteInst],
    stations: &[StationSpec],
) -> Vec<Vec<Vec2>> {
    let stops_at = |stops: &[StationId]| -> Option<Vec<Vec2>> {
        stops
            .iter()
            .map(|s| stations.get(usize::from(s.0)).map(grid_pos))
            .collect()
    };
    pending
        .iter()
        .filter_map(|cmd| match cmd {
            Command::AddRoute { stops } | Command::EditRoute { stops, .. } => stops_at(stops),
            Command::RemoveRoute { route } => routes
                .iter()
                .find(|r| r.id == *route)
                .map(|r| r.points.clone()),
        })
        .collect()
}

pub fn ghost_geometry(draw: &PendingDraw) -> Geometry {
    let mut g = Geometry::default();
    for line in &draw.lines {
        for pair in line.windows(2) {
            g.segment(
                to_world(pair[0]),
                to_world(pair[1]),
                GHOST_HALF,
                GHOST_COLOUR,
            );
        }
    }
    g
}

pub fn buses_layer(bus: &BusDraw, hud: &HudDraw) -> Geometry {
    let mut g = bus_geometry(bus);
    g.append(hud_geometry(hud));
    g
}

pub fn routes_layer(routes: &RouteDraw, pending: &PendingDraw) -> Geometry {
    let mut g = route_geometry(routes);
    g.append(ghost_geometry(pending));
    g
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    Routes,
    Stations,
    Passengers,
    Buses,
}

impl Layer {
    pub const ALL: [Layer; 4] = [
        Layer::Routes,
        Layer::Stations,
        Layer::Passengers,
        Layer::Buses,
    ];

    pub fn z(self) -> f32 {
        match self {
            Layer::Routes => 0.0,
            Layer::Stations => 1.0,
            Layer::Passengers => 2.0,
            Layer::Buses => 3.0,
        }
    }
}

pub fn sync_views(sim: Res<SimRes>, mut scene: ResMut<Scene>, mut routes_draw: ResMut<RouteDraw>) {
    let view = sim.sim.view();
    let restarted = scene.tick.is_some_and(|t| view.tick.0 < t.0);
    if restarted || routes_draw.rev != Some(view.table_rev) {
        *routes_draw = RouteDraw {
            rev: Some(view.table_rev),
            routes: routes(&sim.sim.snapshot(), &sim.scenario.stations),
        };
    }
    *scene = self::scene(&view, &sim.scenario.stations);
}

pub fn sync_pending(
    sim: Res<SimRes>,
    pending: Res<PendingCommands>,
    routes_draw: Res<RouteDraw>,
    mut draw: ResMut<PendingDraw>,
) {
    draw.set_if_neq(PendingDraw {
        lines: pending_lines(&pending.0, &routes_draw.routes, &sim.scenario.stations),
    });
}

pub fn interpolate_buses(
    track: Res<BusTrack>,
    scene: Res<Scene>,
    fixed: Res<Time<Fixed>>,
    mut draw: ResMut<BusDraw>,
    mut held: Local<(u64, f32)>,
) {
    let raw = fixed.overstep_fraction();
    let alpha = if held.0 == track.tick().0 {
        raw.max(held.1)
    } else {
        raw
    };
    *held = (track.tick().0, alpha);
    let at = track.at(alpha);
    draw.set_if_neq(BusDraw {
        buses: buses(&at, &scene.riders),
    });
}

fn fit_camera(
    sim: Res<SimRes>,
    mut cams: Query<(&mut Projection, &mut Transform), With<Camera2d>>,
) {
    let (centre, size) = camera_fit(&sim.scenario);
    for (mut proj, mut tf) in &mut cams {
        let fitted = matches!(
            &*proj,
            Projection::Orthographic(o) if matches!(
                o.scaling_mode,
                ScalingMode::AutoMin { min_width, min_height } if min_width == size.x && min_height == size.y
            )
        );
        if !fitted {
            *proj = Projection::Orthographic(OrthographicProjection {
                scaling_mode: ScalingMode::AutoMin {
                    min_width: size.x,
                    min_height: size.y,
                },
                ..OrthographicProjection::default_2d()
            });
        }
        if tf.translation.truncate() != centre {
            tf.translation.x = centre.x;
            tf.translation.y = centre.y;
        }
    }
}

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct ShapeMaterial {
    #[uniform(0)]
    pub tint: LinearRgba,
}

impl Material2d for ShapeMaterial {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Path(
            AssetPath::from_path_buf(embedded_path!("shapes.wesl")).with_source("embedded"),
        )
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

fn spawn_layers(
    mut commands: Commands,
    meshes: Res<Assets<Mesh>>,
    mut materials: ResMut<Assets<ShapeMaterial>>,
) {
    let material = materials.add(ShapeMaterial {
        tint: LinearRgba::WHITE,
    });
    for layer in Layer::ALL {
        commands.spawn((
            layer,
            Mesh2d(meshes.reserve_handle()),
            MeshMaterial2d(material.clone()),
            Transform::from_xyz(0.0, 0.0, layer.z()),
            Visibility::Hidden,
            NoFrustumCulling,
        ));
    }
}

fn upload(
    layer: Layer,
    geometry: Geometry,
    meshes: &mut Assets<Mesh>,
    layers: &mut Query<(&Layer, &Mesh2d, &mut Visibility)>,
) {
    for (l, mesh, mut vis) in layers.iter_mut() {
        if *l != layer {
            continue;
        }
        if geometry.indices.is_empty() {
            *vis = Visibility::Hidden;
            continue;
        }
        let m = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, geometry.positions.clone())
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, geometry.colors.clone())
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, geometry.uvs.clone())
        .with_inserted_indices(Indices::U32(geometry.indices.clone()));
        meshes
            .insert(&mesh.0, m)
            .expect("layer mesh handle is live");
        *vis = Visibility::Inherited;
    }
}

fn upload_routes(
    draw: Res<RouteDraw>,
    pending: Res<PendingDraw>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut layers: Query<(&Layer, &Mesh2d, &mut Visibility)>,
) {
    upload(
        Layer::Routes,
        routes_layer(&draw, &pending),
        &mut meshes,
        &mut layers,
    );
}

fn upload_scene(
    scene: Res<Scene>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut layers: Query<(&Layer, &Mesh2d, &mut Visibility)>,
) {
    upload(
        Layer::Stations,
        station_geometry(&scene),
        &mut meshes,
        &mut layers,
    );
    upload(
        Layer::Passengers,
        passenger_geometry(&scene),
        &mut meshes,
        &mut layers,
    );
}

fn upload_buses(
    draw: Res<BusDraw>,
    hud: Res<HudDraw>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut layers: Query<(&Layer, &Mesh2d, &mut Visibility)>,
) {
    upload(
        Layer::Buses,
        buses_layer(&draw, &hud),
        &mut meshes,
        &mut layers,
    );
}

pub struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shapes.wesl");
        app.add_plugins(Material2dPlugin::<ShapeMaterial>::default())
            .init_resource::<Scene>()
            .init_resource::<RouteDraw>()
            .init_resource::<BusDraw>()
            .init_resource::<HudDraw>()
            .init_resource::<PendingDraw>()
            .add_systems(Startup, spawn_layers)
            .add_systems(
                Update,
                (
                    sync_views.run_if(resource_exists_and_changed::<SimRes>),
                    sync_pending
                        .after(read_pointer)
                        .run_if(resource_exists::<SimRes>),
                    interpolate_buses,
                    fit_camera.run_if(resource_exists::<SimRes>),
                    upload_routes.run_if(
                        resource_changed::<RouteDraw>.or_else(resource_changed::<PendingDraw>),
                    ),
                    upload_scene.run_if(resource_changed::<Scene>),
                    upload_buses
                        .run_if(resource_changed::<BusDraw>.or_else(resource_changed::<HudDraw>)),
                )
                    .chain(),
            );
    }
}
