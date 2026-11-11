use crate::bus::BusTrack;
use crate::input::SimRes;
use bevy::asset::RenderAssetUsages;
use bevy::camera::ScalingMode;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use salmong_core::ids::{BusId, RouteId, StationId, Tick};
use salmong_core::network::NetworkSnapshot;
use salmong_core::scenario::{Scenario, Shape, StationSpec};
use salmong_core::sim::SimView;
use std::f32::consts::{FRAC_PI_2, TAU};

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
    pub indices: Vec<u32>,
}

impl Geometry {
    fn vertex(&mut self, p: Vec2, colour: [f32; 4]) -> u32 {
        self.positions.push([p.x, p.y, 0.0]);
        self.colors.push(colour);
        (self.positions.len() - 1) as u32
    }

    fn fan(&mut self, centre: Vec2, rim: &[Vec2], colour: Srgba) {
        let c = LinearRgba::from(colour).to_f32_array();
        let base = self.vertex(centre, c);
        for &p in rim {
            self.vertex(centre + p, c);
        }
        let n = rim.len() as u32;
        for i in 0..n {
            self.indices
                .extend([base, base + 1 + i, base + 1 + (i + 1) % n]);
        }
    }

    fn convex(&mut self, centre: Vec2, corners: &[Vec2], colour: Srgba) {
        let c = LinearRgba::from(colour).to_f32_array();
        let base = self.positions.len() as u32;
        for &p in corners {
            self.vertex(centre + p, c);
        }
        for i in 1..corners.len() as u32 - 1 {
            self.indices.extend([base, base + i, base + i + 1]);
        }
    }

    fn quad(&mut self, centre: Vec2, half: Vec2, colour: Srgba) {
        self.convex(
            centre,
            &[
                Vec2::new(-half.x, -half.y),
                Vec2::new(half.x, -half.y),
                Vec2::new(half.x, half.y),
                Vec2::new(-half.x, half.y),
            ],
            colour,
        );
    }

    fn shape(&mut self, shape: Shape, centre: Vec2, r: f32, colour: Srgba) {
        let polar = |n: usize, k: usize, radius: f32| {
            let a = FRAC_PI_2 + TAU * k as f32 / n as f32;
            Vec2::new(a.cos(), a.sin()) * radius
        };
        match shape {
            Shape::Circle => {
                let rim: Vec<Vec2> = (0..16).map(|k| polar(16, k, r)).collect();
                self.fan(centre, &rim, colour);
            }
            Shape::Triangle => {
                let pts: Vec<Vec2> = (0..3).map(|k| polar(3, k, r)).collect();
                self.convex(centre, &pts, colour);
            }
            Shape::Square => self.quad(centre, Vec2::splat(r), colour),
            Shape::Star => {
                let rim: Vec<Vec2> = (0..10)
                    .map(|k| polar(10, k, if k % 2 == 0 { r } else { r * 0.45 }))
                    .collect();
                self.fan(centre, &rim, colour);
            }
        }
    }

    fn segment(&mut self, a: Vec2, b: Vec2, half: f32, colour: Srgba) {
        let n = (b - a).normalize_or_zero().perp() * half;
        self.convex(Vec2::ZERO, &[a + n, a - n, b - n, b + n], colour);
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
        g.quad(to_world(b.pos), Vec2::splat(BADGE_HALF), BADGE_COLOUR);
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
        g.quad(c, BUS_HALF, BUS_COLOUR);
        for k in 0..b.riders.min(RIDERS_SHOWN) {
            g.quad(c + rider_offset(k), Vec2::splat(RIDER_HALF), RIDER_COLOUR);
        }
    }
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

pub fn interpolate_buses(
    track: Res<BusTrack>,
    scene: Res<Scene>,
    fixed: Res<Time<Fixed>>,
    mut draw: ResMut<BusDraw>,
) {
    let at = track.at(fixed.overstep_fraction());
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

fn spawn_layers(
    mut commands: Commands,
    meshes: Res<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let material = materials.add(ColorMaterial::from_color(Color::WHITE));
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
        .with_inserted_indices(Indices::U32(geometry.indices.clone()));
        meshes
            .insert(&mesh.0, m)
            .expect("layer mesh handle is live");
        *vis = Visibility::Inherited;
    }
}

fn upload_routes(
    draw: Res<RouteDraw>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut layers: Query<(&Layer, &Mesh2d, &mut Visibility)>,
) {
    upload(
        Layer::Routes,
        route_geometry(&draw),
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
    mut meshes: ResMut<Assets<Mesh>>,
    mut layers: Query<(&Layer, &Mesh2d, &mut Visibility)>,
) {
    upload(Layer::Buses, bus_geometry(&draw), &mut meshes, &mut layers);
}

pub struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Scene>()
            .init_resource::<RouteDraw>()
            .init_resource::<BusDraw>()
            .add_systems(Startup, spawn_layers)
            .add_systems(
                Update,
                (
                    sync_views.run_if(resource_exists_and_changed::<SimRes>),
                    interpolate_buses,
                    fit_camera.run_if(resource_exists::<SimRes>),
                    upload_routes.run_if(resource_changed::<RouteDraw>),
                    upload_scene.run_if(resource_changed::<Scene>),
                    upload_buses.run_if(resource_changed::<BusDraw>),
                )
                    .chain(),
            );
    }
}
