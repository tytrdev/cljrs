//! 2D physics bindings via rapier2d.

use cljrs::env::Env;
use cljrs::error::{Error, Result};
use cljrs::value::Value;
use rapier2d::prelude::*;
use std::sync::Mutex;

use crate::{
    arg_map, arg_u32, arg_world, as_f32, as_kw, bind, f32_vec, map_get, opaque, vec_components,
};

const TAG: &str = "physics2d/world";

pub struct World2 {
    gravity: Vector<Real>,
    integration: IntegrationParameters,
    islands: IslandManager,
    broad_phase: DefaultBroadPhase,
    narrow_phase: NarrowPhase,
    bodies: RigidBodySet,
    colliders: ColliderSet,
    impulse_joints: ImpulseJointSet,
    multibody_joints: MultibodyJointSet,
    ccd: CCDSolver,
    query_pipeline: QueryPipeline,
    pipeline: PhysicsPipeline,
}

impl World2 {
    fn new(gravity: Vector<Real>) -> Self {
        Self {
            gravity,
            integration: IntegrationParameters::default(),
            islands: IslandManager::new(),
            broad_phase: DefaultBroadPhase::new(),
            narrow_phase: NarrowPhase::new(),
            bodies: RigidBodySet::new(),
            colliders: ColliderSet::new(),
            impulse_joints: ImpulseJointSet::new(),
            multibody_joints: MultibodyJointSet::new(),
            ccd: CCDSolver::new(),
            query_pipeline: QueryPipeline::new(),
            pipeline: PhysicsPipeline::new(),
        }
    }

    fn step(&mut self) {
        let hooks = ();
        let events = ();
        self.pipeline.step(
            &self.gravity,
            &self.integration,
            &mut self.islands,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.bodies,
            &mut self.colliders,
            &mut self.impulse_joints,
            &mut self.multibody_joints,
            &mut self.ccd,
            Some(&mut self.query_pipeline),
            &hooks,
            &events,
        );
    }
}

pub fn install(env: &Env) {
    bind(env, "world", world_fn);
    bind(env, "add-body!", add_body_fn);
    bind(env, "step!", step_fn);
    bind(env, "translation", translation_fn);
    bind(env, "rotation", rotation_fn);
    bind(env, "linvel", linvel_fn);
    bind(env, "set-linvel!", set_linvel_fn);
    bind(env, "apply-impulse!", apply_impulse_fn);
    bind(env, "body-count", body_count_fn);
    bind(env, "remove-body!", remove_body_fn);
}

fn world_fn(args: &[Value]) -> Result<Value> {
    let gravity = if args.is_empty() {
        vector![0.0, -9.81]
    } else {
        let m = arg_map(args, 0, "world")?;
        match map_get(m, "gravity") {
            Some(v) => {
                let c = vec_components(v)?;
                if c.len() != 2 {
                    return Err(Error::Type(format!(
                        "world: :gravity must be [x y], got {} components",
                        c.len()
                    )));
                }
                vector![c[0], c[1]]
            }
            None => vector![0.0, -9.81],
        }
    };
    Ok(opaque(TAG, World2::new(gravity)))
}

fn add_body_fn(args: &[Value]) -> Result<Value> {
    let w = arg_world::<World2>(args, 0, TAG, "add-body!")?;
    let m = arg_map(args, 1, "add-body!")?;

    let body_type = match map_get(m, "type") {
        Some(v) => as_kw(v)?.to_string(),
        None => "dynamic".to_string(),
    };
    let position = match map_get(m, "position") {
        Some(v) => {
            let c = vec_components(v)?;
            if c.len() != 2 {
                return Err(Error::Type(
                    "add-body!: :position must be [x y]".into(),
                ));
            }
            [c[0], c[1]]
        }
        None => [0.0, 0.0],
    };
    let rotation = match map_get(m, "rotation") {
        Some(v) => as_f32(v)?,
        None => 0.0,
    };
    let linvel = match map_get(m, "linvel") {
        Some(v) => {
            let c = vec_components(v)?;
            if c.len() != 2 {
                return Err(Error::Type("add-body!: :linvel must be [x y]".into()));
            }
            [c[0], c[1]]
        }
        None => [0.0, 0.0],
    };

    let builder = match body_type.as_str() {
        "dynamic" => RigidBodyBuilder::dynamic(),
        "fixed" | "static" => RigidBodyBuilder::fixed(),
        "kinematic" => RigidBodyBuilder::kinematic_position_based(),
        other => {
            return Err(Error::Type(format!(
                "add-body!: unknown :type :{other} (want :dynamic/:fixed/:kinematic)"
            )));
        }
    };
    let rb = builder
        .translation(vector![position[0], position[1]])
        .rotation(rotation)
        .linvel(vector![linvel[0], linvel[1]])
        .build();

    let collider = match map_get(m, "collider") {
        Some(Value::Map(cm)) => Some(build_collider(cm)?),
        Some(v) => {
            return Err(Error::Type(format!(
                "add-body!: :collider must be map, got {}",
                v.type_name()
            )));
        }
        None => None,
    };

    let mut w = w.lock().unwrap();
    let w = &mut *w;
    let handle = w.bodies.insert(rb);
    if let Some(c) = collider {
        w.colliders.insert_with_parent(c, handle, &mut w.bodies);
    }
    Ok(Value::Int(handle.into_raw_parts().0 as i64))
}

fn build_collider(m: &imbl::HashMap<Value, Value>) -> Result<Collider> {
    let shape = match map_get(m, "shape") {
        Some(v) => as_kw(v)?.to_string(),
        None => return Err(Error::Type("collider: missing :shape".into())),
    };
    let restitution = match map_get(m, "restitution") {
        Some(v) => as_f32(v)?,
        None => 0.0,
    };
    let friction = match map_get(m, "friction") {
        Some(v) => as_f32(v)?,
        None => 0.5,
    };
    let density = match map_get(m, "density") {
        Some(v) => as_f32(v)?,
        None => 1.0,
    };
    let builder = match shape.as_str() {
        "ball" => {
            let r = match map_get(m, "radius") {
                Some(v) => as_f32(v)?,
                None => {
                    return Err(Error::Type(
                        "collider :shape :ball needs :radius".into(),
                    ));
                }
            };
            ColliderBuilder::ball(r)
        }
        "box" | "cuboid" => {
            let half = match map_get(m, "half-extents") {
                Some(v) => vec_components(v)?,
                None => return Err(Error::Type("collider :box needs :half-extents".into())),
            };
            if half.len() != 2 {
                return Err(Error::Type(
                    "collider :box :half-extents must be [hx hy]".into(),
                ));
            }
            ColliderBuilder::cuboid(half[0], half[1])
        }
        other => {
            return Err(Error::Type(format!(
                "collider: unknown :shape :{other} (want :ball/:box)"
            )));
        }
    };
    Ok(builder
        .restitution(restitution)
        .friction(friction)
        .density(density)
        .build())
}

fn step_fn(args: &[Value]) -> Result<Value> {
    let w = arg_world::<World2>(args, 0, TAG, "step!")?;
    w.lock().unwrap().step();
    Ok(Value::Nil)
}

fn handle_from(idx: u32, bodies: &RigidBodySet) -> Option<RigidBodyHandle> {
    bodies
        .iter()
        .map(|(h, _)| h)
        .find(|h| h.into_raw_parts().0 == idx)
}

fn with_body<F, R>(
    args: &[Value],
    name: &str,
    f: F,
) -> Result<R>
where
    F: FnOnce(&mut World2, RigidBodyHandle) -> Result<R>,
{
    let w = arg_world::<World2>(args, 0, TAG, name)?;
    let idx = arg_u32(args, 1, name)?;
    let mut w = w.lock().unwrap();
    let h = handle_from(idx, &w.bodies).ok_or_else(|| {
        Error::Eval(format!("{name}: no body with index {idx}"))
    })?;
    f(&mut w, h)
}

fn translation_fn(args: &[Value]) -> Result<Value> {
    with_body(args, "translation", |w, h| {
        let b = &w.bodies[h];
        let t = b.translation();
        Ok(f32_vec(&[t.x, t.y]))
    })
}

fn rotation_fn(args: &[Value]) -> Result<Value> {
    with_body(args, "rotation", |w, h| {
        let b = &w.bodies[h];
        Ok(Value::Float(b.rotation().angle() as f64))
    })
}

fn linvel_fn(args: &[Value]) -> Result<Value> {
    with_body(args, "linvel", |w, h| {
        let b = &w.bodies[h];
        let v = b.linvel();
        Ok(f32_vec(&[v.x, v.y]))
    })
}

fn set_linvel_fn(args: &[Value]) -> Result<Value> {
    let vel = match args.get(2) {
        Some(v) => {
            let c = vec_components(v)?;
            if c.len() != 2 {
                return Err(Error::Type("set-linvel!: [vx vy]".into()));
            }
            vector![c[0], c[1]]
        }
        None => return Err(Error::Eval("set-linvel!: missing velocity".into())),
    };
    with_body(args, "set-linvel!", |w, h| {
        if let Some(b) = w.bodies.get_mut(h) {
            b.set_linvel(vel, true);
        }
        Ok(Value::Nil)
    })
}

fn apply_impulse_fn(args: &[Value]) -> Result<Value> {
    let imp = match args.get(2) {
        Some(v) => {
            let c = vec_components(v)?;
            if c.len() != 2 {
                return Err(Error::Type("apply-impulse!: [ix iy]".into()));
            }
            vector![c[0], c[1]]
        }
        None => return Err(Error::Eval("apply-impulse!: missing impulse".into())),
    };
    with_body(args, "apply-impulse!", |w, h| {
        if let Some(b) = w.bodies.get_mut(h) {
            b.apply_impulse(imp, true);
        }
        Ok(Value::Nil)
    })
}

fn body_count_fn(args: &[Value]) -> Result<Value> {
    let w = arg_world::<World2>(args, 0, TAG, "body-count")?;
    let n = w.lock().unwrap().bodies.len();
    Ok(Value::Int(n as i64))
}

fn remove_body_fn(args: &[Value]) -> Result<Value> {
    with_body(args, "remove-body!", |w, h| {
        w.bodies.remove(
            h,
            &mut w.islands,
            &mut w.colliders,
            &mut w.impulse_joints,
            &mut w.multibody_joints,
            true,
        );
        Ok(Value::Nil)
    })
}

// Silence unused-import warning for Mutex; used via arg_world indirectly.
#[allow(dead_code)]
fn _use_mutex() -> Mutex<()> {
    Mutex::new(())
}
