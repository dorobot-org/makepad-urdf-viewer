//! Instanced lit mesh shader for robot links (makepad dev script system).
//!
//! Same pattern as makepad's examples/box3d DrawPhysMesh: non-instance data
//! before #[deref], per-instance fields after.

use makepad_widgets::*;

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    use mod.math.*
    use mod.shader.*
    use mod.draw
    use mod.geom

    // Depth-only pass from the light's point of view. Writes linear distance
    // along the light axis into a float colour target — makepad binds
    // DepthD32 as a pass attachment and has no path to sample it as a
    // texture, so the shadow map has to be a colour render target.
    mod.draw.DrawShadowDepth = mod.std.set_type_default() do #(DrawShadowDepth::script_shader(vm)){
        alpha_blend: false
        backface_culling: false
        vertex_pos: vertex_position(vec4f)
        fb0: fragment_output(0, vec4f)
        draw_pass: uniform_buffer(draw.DrawPassUniforms)
        draw_list: uniform_buffer(draw.DrawListUniforms)
        geom: vertex_buffer(geom.IcoVertex, geom.IcoGeom)
        v_depth: varying(f32)

        vertex: fn() {
            let local_pos = vec3(
                self.geom.pos.x * self.scale.x,
                self.geom.pos.y * self.scale.y,
                self.geom.pos.z * self.scale.z
            )
            let world = self.transform * vec4(local_pos.x, local_pos.y, local_pos.z, 1.0)
            let clip = self.light_vp * vec4(world.x, world.y, world.z, 1.0)
            // glam's orthographic_rh already yields z in [0,1] (Metal/wgpu
            // convention) — mapping it again with *0.5+0.5 squeezed the whole
            // scene into the top half of an 8-bit target.
            self.v_depth = clip.z
            self.vertex_pos = clip
        }

        pixel: fn() -> vec4f {
            // 16-bit depth packed into G (hi byte) and A (lo byte): an 8-bit
            // map quantises at 1/255 = 0.004, the same size as a usable bias,
            // so single-channel depth acnes over every surface. G and A are
            // the two channels a BGRA/RGBA swap leaves alone, so the encoding
            // survives whichever order the backend hands back.
            // Alpha must stay 1.0: the pass blends premultiplied even with
            // alpha_blend:false (harmless for every shader that writes a=1 —
            // blending degenerates to replace — but data in alpha gets mixed
            // with the clear colour; measured as depth-contour scribbles in
            // the map). Lo rides in R with B zero, so the r/b swap on other
            // backends still yields lo as smp.x + smp.z.
            let d = clamp(self.v_depth, 0.0, 1.0)
            let hi = floor(d * 255.0) / 255.0
            let lo = clamp(fract(d * 255.0), 0.0, 1.0)
            return vec4(lo, hi, 0.0, 1.0)
        }

        fragment: fn() {
            self.fb0 = self.pixel()
        }
    }

    // Planar reflection of the robot on the ground plane — MuJoCo's
    // reflectance look. Same vertex path as the lit mesh (the reflection
    // matrix rides in `transform`); lighting is the hemisphere + key only,
    // blended over the already-drawn ground at `reflect_alpha`, premultiplied
    // for the ONE / ONE_MINUS_SRC_ALPHA pass blend.
    mod.draw.DrawRobotReflect = mod.std.set_type_default() do #(DrawRobotReflect::script_shader(vm)){
        alpha_blend: true
        depth_write: false
        backface_culling: false
        vertex_pos: vertex_position(vec4f)
        fb0: fragment_output(0, vec4f)
        draw_call: uniform_buffer(draw.DrawCallUniforms)
        draw_pass: uniform_buffer(draw.DrawPassUniforms)
        draw_list: uniform_buffer(draw.DrawListUniforms)
        geom: vertex_buffer(geom.IcoVertex, geom.IcoGeom)
        v_world: varying(vec3f)
        v_normal: varying(vec3f)

        vertex: fn() {
            let local_pos = vec3(
                self.geom.pos.x * self.scale.x,
                self.geom.pos.y * self.scale.y,
                self.geom.pos.z * self.scale.z
            )
            let world = self.transform * vec4(local_pos.x, local_pos.y, local_pos.z, 1.0)
            let world_normal = normalize((self.transform * vec4(self.geom.normal.x, self.geom.normal.y, self.geom.normal.z, 0.0)).xyz)
            self.v_world = world.xyz
            self.v_normal = world_normal
            let view_pos = self.draw_pass.camera_view * world
            self.vertex_pos = self.draw_pass.camera_projection * view_pos
        }

        pixel: fn() -> vec4f {
            let camera_world = self.draw_pass.camera_inv * vec4(0.0, 0.0, 0.0, 1.0)
            let cw = vec3(camera_world.x, camera_world.y, camera_world.z)
            let view_dir = normalize(cw - self.v_world)
            let normal_in = normalize(self.v_normal)
            let normal = normal_in * sign(dot(normal_in, view_dir))
            let key_dir = normalize(self.key_light.xyz)
            let sky = self.ambient_sky.xyz
            let ground = self.ambient_ground.xyz
            let hemi = mix(ground, sky, normal.y * 0.5 + 0.5)
            let key = max(dot(normal, key_dir), 0.0)
            let head = max(dot(normal, view_dir), 0.0) * 0.55
            let kg = 0.35 + 0.55 * self.key_light.w
            let lit_r = self.color.x * (hemi.x + key * kg + head)
            let lit_g = self.color.y * (hemi.y + key * kg + head)
            let lit_b = self.color.z * (hemi.z + key * kg + head)
            let a = self.reflect_alpha
            return vec4(lit_r * a, lit_g * a, lit_b * a, a)
        }

        fragment: fn() {
            self.fb0 = self.pixel()
        }
    }

    mod.draw.DrawRobotMesh = mod.std.set_type_default() do #(DrawRobotMesh::script_shader(vm)){
        alpha_blend: false
        backface_culling: false
        vertex_pos: vertex_position(vec4f)
        fb0: fragment_output(0, vec4f)
        draw_call: uniform_buffer(draw.DrawCallUniforms)
        draw_pass: uniform_buffer(draw.DrawPassUniforms)
        draw_list: uniform_buffer(draw.DrawListUniforms)
        geom: vertex_buffer(geom.IcoVertex, geom.IcoGeom)
        shadow_map: texture_2d(float)
        v_world_clip: varying(vec4f)
        v_world: varying(vec3f)
        v_normal: varying(vec3f)

        active_camera_world_pos: fn() -> vec3f {
            let camera_world = self.draw_pass.camera_inv * vec4(0.0, 0.0, 0.0, 1.0)
            return vec3(
                camera_world.x / max(camera_world.w, 0.00001),
                camera_world.y / max(camera_world.w, 0.00001),
                camera_world.z / max(camera_world.w, 0.00001)
            )
        }

        vertex: fn() {
            let local_pos = vec3(
                self.geom.pos.x * self.scale.x,
                self.geom.pos.y * self.scale.y,
                self.geom.pos.z * self.scale.z
            )
            let local_normal = normalize(vec3(
                self.geom.normal.x / max(self.scale.x, 0.00001),
                self.geom.normal.y / max(self.scale.y, 0.00001),
                self.geom.normal.z / max(self.scale.z, 0.00001)
            ))
            let model_view = self.draw_list.view_transform * self.transform
            let world = model_view * vec4(local_pos.x, local_pos.y, local_pos.z, 1.0)
            let world_normal = normalize((model_view * vec4(local_normal.x, local_normal.y, local_normal.z, 0.0)).xyz)
            self.v_world = world.xyz
            self.v_normal = world_normal
            self.v_world_clip = vec4(world.x, world.y, world.z, 1.0)
            let view_pos = self.draw_pass.camera_view * world
            self.vertex_pos = self.draw_pass.camera_projection * view_pos
        }

        pixel: fn() {
            let normal_in = normalize(self.v_normal)
            let view_dir = normalize(self.active_camera_world_pos() - self.v_world)
            // flip normals on backfaces so double-sided meshes shade correctly
            let normal = normal_in * sign(dot(normal_in, view_dir))
            // key = the draggable lamp (xyz direction, w = lamp switched on)
            let key_dir = normalize(self.key_light.xyz)
            let lamp = self.key_light.w
            let fill_dir = normalize(vec3(0.58, 0.35, -0.62))

            // Hemisphere ambient, driven by the SAME colours the composite
            // paints the environment with (per-instance, because set_uniform
            // is unreliable on wasm). Hardcoding it here meant re-theming the
            // sky left objects lit for the old one — a lit robot floating in
            // an environment it visibly does not belong to.
            let sky = self.ambient_sky.xyz
            let ground = self.ambient_ground.xyz
            let hemi = mix(ground, sky, normal.y * 0.5 + 0.5)

            // Shadow lookup: project the fragment into the light's clip box
            // and compare against the nearest surface the light saw. 3x3 PCF
            // because this dialect has no textureGather, and a slope-scaled
            // bias because grazing surfaces self-shadow otherwise (there is
            // no fwidth here either, hence the explicit N.L term).
            let lit = 1.0 - clamp(dot(normal, key_dir), 0.0, 1.0)
            let bias = self.shadow_texel.z + self.shadow_texel.w * lit
            let sc = self.light_vp * vec4(self.v_world.x, self.v_world.y, self.v_world.z, 1.0)
            // Render-target V orientation differs by backend; carry it as a
            // value so it can be verified rather than assumed.
            let suv = vec2(sc.x * 0.5 + 0.5,
                mix(sc.y * 0.5 + 0.5, 0.5 - sc.y * 0.5, self.shadow_flip))
            let sdepth = sc.z
            let mut shade = 0.0
            let mut taps = 0.0
            // Outside the map (or behind the light) is lit, never shadowed —
            // the map only covers a box around the robot.
            if suv.x > 0.001 && suv.x < 0.999 && suv.y > 0.001 && suv.y < 0.999 && sdepth < 1.0 {
                for oy in 0..3 {
                    for ox in 0..3 {
                        let o = vec2(
                            (float(ox) - 1.0) * self.shadow_texel.x,
                            (float(oy) - 1.0) * self.shadow_texel.y
                        )
                        // Nearest, not linear: filtering a two-channel packed
                        // depth interpolates hi and lo independently and
                        // invents depths where the hi byte steps.
                        let smp = self.shadow_map.sample_nearest(vec2(suv.x + o.x, suv.y + o.y))
                        let occluder = smp.y + (smp.x + smp.z) / 255.0
                        shade = shade + step(occluder + bias, sdepth)
                        taps = taps + 1.0
                    }
                }
                shade = shade / max(taps, 1.0)
            }
            // NOT gated on the lamp switch: the key light still contributes
            // 35% with the lamp off (see key_gain below), so gating the shadow
            // on it made the lighting and the shadowing disagree — a lit
            // scene with no shadows in it.
            let shadow = 1.0 - shade * self.shadow_strength

            // warm key + cool fill diffuse; the lamp lifts the key
            let key = max(dot(normal, key_dir), 0.0) * shadow
            let fill = max(dot(normal, fill_dir), 0.0)
            // Camera headlight, MuJoCo's dominant light: white diffuse from
            // the eye (mjModel.vis.headlight diffuse ~0.6). Without it the
            // camera-facing surfaces sit in ambient only and the robot reads
            // charcoal in the dark-sky palette.
            let head = max(dot(normal, view_dir), 0.0)
            let key_gain = 0.35 + 0.55 * lamp
            let diffuse = self.color.xyz * (
                hemi
                + key * vec3(1.0, 0.95, 0.86) * key_gain
                + fill * vec3(0.62, 0.64, 0.70) * 0.22
                + head * vec3(0.55, 0.55, 0.55)
            )

            // blinn specular from both lights
            let half_key = normalize(key_dir + view_dir)
            let half_fill = normalize(fill_dir + view_dir)
            let spec_gain = (0.80 + 0.60 * lamp) * shadow
            let spec = (pow(max(dot(normal, half_key), 0.0), 48.0) * 0.45
                + pow(max(dot(normal, half_fill), 0.0), 24.0) * 0.12) * spec_gain

            // cool rim to lift silhouettes off the dark background
            let rim = pow(max(1.0 - max(dot(normal, view_dir), 0.0), 0.0), 3.0)

            let color = diffuse
                + vec3(1.0, 0.98, 0.95) * spec
                + vec3(0.26, 0.23, 0.20) * rim
            return vec4(color, self.color.w)
        }

        fragment: fn() {
            self.fb0 = depth_clip(self.v_world_clip, self.pixel(), self.depth_clip)
        }
    }

    mod.draw.DrawGridPlane = mod.std.set_type_default() do #(DrawGridPlane::script_shader(vm)){
        alpha_blend: true
        depth_write: false
        backface_culling: false
        vertex_pos: vertex_position(vec4f)
        fb0: fragment_output(0, vec4f)
        draw_call: uniform_buffer(draw.DrawCallUniforms)
        draw_pass: uniform_buffer(draw.DrawPassUniforms)
        draw_list: uniform_buffer(draw.DrawListUniforms)
        geom: vertex_buffer(geom.IcoVertex, geom.IcoGeom)
        shadow_map: texture_2d(float)
        v_world: varying(vec3f)
        v_world_clip: varying(vec4f)

        cam_world: fn() -> vec3f {
            let cw = self.draw_pass.camera_inv * vec4(0.0, 0.0, 0.0, 1.0)
            let iw = 1.0 / max(cw.w, 0.00001)
            return vec3(cw.x * iw, cw.y * iw, cw.z * iw)
        }

        vertex: fn() {
            let world = vec4(
                self.geom.pos.x * self.extent,
                self.plane_y,
                self.geom.pos.z * self.extent,
                1.0
            )
            self.v_world = world.xyz
            self.v_world_clip = world
            let view_pos = self.draw_pass.camera_view * world
            self.vertex_pos = self.draw_pass.camera_projection * view_pos
        }

        pixel: fn() {
            // distance (world units) to the nearest minor / major grid line
            let gx = abs(fract(self.v_world.x / self.spacing + 0.5) - 0.5) * self.spacing
            let gz = abs(fract(self.v_world.z / self.spacing + 0.5) - 0.5) * self.spacing
            let s5 = self.spacing * 5.0
            let g5x = abs(fract(self.v_world.x / s5 + 0.5) - 0.5) * s5
            let g5z = abs(fract(self.v_world.z / s5 + 0.5) - 0.5) * s5
            // The plane runs to the horizon, so the lines have to be
            // anti-aliased by hand — this dialect has no fwidth. Lines keep a
            // CONSTANT WORLD WIDTH so they get thinner with distance; once one
            // would fall under a pixel it is widened to exactly one pixel and
            // its opacity scaled by how much it lost, so it reads as thinner
            // and fainter rather than as a blurred band.
            let cam = self.cam_world()
            let to_frag = self.v_world - cam
            let dist = max(length(to_frag), 0.0001)
            let cam_h = max(abs(cam.y - self.plane_y), 0.001)
            // a pixel's footprint on the plane is anisotropic: transverse to
            // the view ray it is dist * px_scale, along the ray it is stretched
            // by 1 / sin(grazing) = dist / camera_height
            let px_t = max(dist * self.px_scale, 0.0000001)
            let px_l = px_t * min(dist / cam_h, 200.0)
            let horiz_len = max(length(vec2(to_frag.x, to_frag.z)), 0.00001)
            let hx = abs(to_frag.x) / horiz_len
            let hz2 = abs(to_frag.z) / horiz_len
            // footprint measured across each axis family
            let fx = hx * px_l + hz2 * px_t
            let fz = hz2 * px_l + hx * px_t
            let lw = self.spacing * 0.010
            let lw5 = self.spacing * 0.022
            let wx = max(lw, fx)
            let wz = max(lw, fz)
            let minor = max(
                (1.0 - smoothstep(0.0, wx, gx)) * clamp(lw / wx, 0.0, 1.0),
                (1.0 - smoothstep(0.0, wz, gz)) * clamp(lw / wz, 0.0, 1.0)
            )
            let wx5 = max(lw5, fx)
            let wz5 = max(lw5, fz)
            let major = max(
                (1.0 - smoothstep(0.0, wx5, g5x)) * clamp(lw5 / wx5, 0.0, 1.0),
                (1.0 - smoothstep(0.0, wz5, g5z)) * clamp(lw5 / wz5, 0.0, 1.0)
            )
            let r = length(vec2(self.v_world.x, self.v_world.z))
            // only a whisper of a fade at the very rim of the quad, which sits
            // far enough out that its edge lands inside the horizon line
            let fade = 1.0 - smoothstep(self.extent * 0.75, self.extent * 0.98, r)
            // No above-the-plane fade: the grid is drawn from both sides, so
            // it still reads when the camera dips under the ground.
            // Pale #FFFFC5 soil with darker lines punching through it — the
            // ground is bright, so the grid reads as shadow, not glow.
            // Scalar arithmetic only: multiplying a let-bound vec3 by a float
            // returns garbage in this script-shader dialect (verified — the
            // same expression written with a vec3 literal renders correctly).
            // MuJoCo-style checker at the major-cell scale: two soil tones
            // alternating per 5x5 minor cell, the same look menagerie's
            // builtin="checker" ground gives the G1 scenes. All scalar math —
            // vec-times-scalar on let-bound vecs is the documented landmine.
            let cpar = floor(self.v_world.x / s5) + floor(self.v_world.z / s5)
            let checker = fract(cpar * 0.5) * 2.0
            let soil_r = mix(self.soil_color.x, self.soil2_color.x, checker)
            let soil_g = mix(self.soil_color.y, self.soil2_color.y, checker)
            let soil_b = mix(self.soil_color.z, self.soil2_color.z, checker)
            let line_a = (minor * 0.55 + major * 0.85) * fade
            let soil_a = 0.85 * fade * (1.0 - line_a)
            let alpha = soil_a + line_a
            // Same haze as the sky dome, applied to the COLOUR only (folding
            // it into alpha instead let the term go out of range and swallowed
            // the whole frame): sin(angle below the horizon) is
            // camera_height / distance, so the plane and the dome fade into
            // the same near-white and the ground stops meeting the sky as a
            // hard saturated band.
            // cam_h/dist is sin(angle below the horizon), matching the dome.
            // Floor the height at a couple of grid cells first: the camera can
            // sit millimetres above the plane (the SO-100 framing puts it at
            // 3.8 mm vs the telescope's 54 mm), and without the floor every
            // fragment counts as grazing and the whole ground hazes to white.
            let haze_h = max(cam_h, self.spacing * 1.5)
            let hz = clamp(1.0 - smoothstep(0.006, 0.060, haze_h / dist), 0.0, 1.0)
            // The ground is where a cast shadow actually reads, so the plane
            // samples the same map the meshes do. Flat and facing straight up,
            // so a constant bias is enough — no slope term needed.
            let sc = self.light_vp * vec4(self.v_world.x, self.v_world.y, self.v_world.z, 1.0)
            let suv = vec2(sc.x * 0.5 + 0.5,
                mix(sc.y * 0.5 + 0.5, 0.5 - sc.y * 0.5, self.shadow_flip))
            let sdepth = sc.z
            let mut shade = 0.0
            let mut taps = 0.0
            if suv.x > 0.001 && suv.x < 0.999 && suv.y > 0.001 && suv.y < 0.999 && sdepth < 1.0 {
                for oy in 0..3 {
                    for ox in 0..3 {
                        let o = vec2(
                            (float(ox) - 1.0) * self.shadow_texel.x,
                            (float(oy) - 1.0) * self.shadow_texel.y
                        )
                        let smp = self.shadow_map.sample_nearest(vec2(suv.x + o.x, suv.y + o.y))
                        let occ = smp.y + (smp.x + smp.z) / 255.0
                        shade = shade + step(occ + self.shadow_texel.z, sdepth)
                        taps = taps + 1.0
                    }
                }
                shade = shade / max(taps, 1.0)
            }
            if self.debug_shadow > 0.5 {
                let c = self.shadow_map.sample(suv).x
                // red = outside the map entirely
                if suv.x < 0.001 || suv.x > 0.999 || suv.y < 0.001 || suv.y > 0.999 {
                    return vec4(1.0, 0.0, 0.0, 1.0)
                }
                return vec4(c, c, sdepth, 1.0)
            }
            // Fades out with the same haze the lines do, so a shadow never
            // hangs in the fog past where the ground itself has faded.
            let sh = 1.0 - shade * self.shadow_strength * (1.0 - hz)
            let soil_s = soil_a * sh
            let line_s = line_a * sh

            // premultiplied output for makepad's ONE / ONE_MINUS_SRC_ALPHA blend
            return vec4(
                mix(soil_r * soil_s + self.color.x * line_s, self.haze_color.x * alpha, hz),
                mix(soil_g * soil_s + self.color.y * line_s, self.haze_color.y * alpha, hz),
                mix(soil_b * soil_s + self.color.z * line_s, self.haze_color.z * alpha, hz),
                alpha
            )
        }

        fragment: fn() {
            self.fb0 = depth_clip(self.v_world_clip, self.pixel(), self.depth_clip)
        }
    }

}

// Environment + composite. The sky dome is evaluated per screen pixel from
// the camera basis (no geometry, no depth interaction) and the scene texture
// — rendered over an alpha-0 clear — is laid on top.
//
// sample_as_bgra returns the render texture already channel-ordered on
// Metal, but R/B-reversed on WebGL (verified both ways against a rendered
// warm grid line). Two shader variants, chosen at compile time — only the
// `scene` line differs.
pub mod composite_shader {
    use super::*;

    #[cfg(not(target_arch = "wasm32"))]
    script_mod! {
        use mod.prelude.widgets.*
        use mod.shader.*
        use mod.draw

        mod.draw.DrawSceneComposite = mod.std.set_type_default() do #(DrawSceneComposite::script_shader(vm)){
            ..mod.draw.DrawQuad
            scene_texture: texture_2d(float)

            pixel: fn() {
                let scene = self.scene_texture.sample_as_bgra(self.pos)
                let ndc_x = self.pos.x * 2.0 - 1.0
                let ndc_y = 1.0 - self.pos.y * 2.0
                let dir = normalize(
                    self.cam_fwd.xyz
                    + self.cam_right.xyz * (ndc_x * self.cam_right.w)
                    + self.cam_up.xyz * (ndc_y * self.cam_up.w)
                )
                let t = dir.y
                let up = clamp(t, 0.0, 1.0)
                // morning: near-white at the horizon, pale yellow just above,
                // soft peach-pink towards the zenith
                // the default framing only shows ~5 deg of sky, so the whole
                // white -> pale yellow -> peach-pink ramp lives near the horizon
                // host-themed: near-white at the horizon lifting to the zenith
                // tint. The whole ramp lives low because the default framing
                // only exposes a few degrees of sky.
                let sky = mix(
                    self.sky_horizon.xyz,
                    self.sky_zenith.xyz,
                    smoothstep(0.0, 0.10, up)
                )
                // the ground below, hazing into the sky as it approaches the
                // horizon — without this the pale yellow meets the pink sky as
                // a hard saturated band
                let down = clamp(-t, 0.0, 1.0)
                let earth0 = mix(
                    self.ground_color.xyz,
                    self.ground_color.xyz * 0.88,
                    smoothstep(0.0, 0.75, down)
                )
                let ground_haze = 1.0 - smoothstep(0.006, 0.060, down)
                let earth = mix(earth0, self.sky_horizon.xyz, ground_haze)
                let glow = pow(1.0 - min(abs(t) * 26.0, 1.0), 3.0)
                let bg0 = mix(earth, sky, smoothstep(-0.010, 0.010, t))
                    + vec3(0.22, 0.20, 0.21) * glow * 0.07
                // the lamp: a small ~0.5 degree disc with a tight halo
                let sd = dot(dir, normalize(self.light_dir.xyz))
                let ang = acos(clamp(sd, -1.0, 1.0))
                let disc = 1.0 - smoothstep(0.0075, 0.0105, ang)
                let halo = (1.0 - smoothstep(0.0, 0.075, ang)) * 0.30
                let bg = bg0 + (vec3(1.0, 0.99, 0.94) * disc
                    + vec3(1.0, 0.88, 0.62) * halo) * self.light_dir.w
                return vec4(
                    scene.x + bg.x * (1.0 - scene.w),
                    scene.y + bg.y * (1.0 - scene.w),
                    scene.z + bg.z * (1.0 - scene.w),
                    1.0
                )
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    script_mod! {
        use mod.prelude.widgets.*
        use mod.shader.*
        use mod.draw

        mod.draw.DrawSceneComposite = mod.std.set_type_default() do #(DrawSceneComposite::script_shader(vm)){
            ..mod.draw.DrawQuad
            scene_texture: texture_2d(float)

            pixel: fn() {
                let s = self.scene_texture.sample_as_bgra(self.pos)
                let scene = vec4(s.z, s.y, s.x, s.w)
                let ndc_x = self.pos.x * 2.0 - 1.0
                let ndc_y = 1.0 - self.pos.y * 2.0
                let dir = normalize(
                    self.cam_fwd.xyz
                    + self.cam_right.xyz * (ndc_x * self.cam_right.w)
                    + self.cam_up.xyz * (ndc_y * self.cam_up.w)
                )
                let t = dir.y
                let up = clamp(t, 0.0, 1.0)
                // morning: near-white at the horizon, pale yellow just above,
                // soft peach-pink towards the zenith
                // the default framing only shows ~5 deg of sky, so the whole
                // white -> pale yellow -> peach-pink ramp lives near the horizon
                // host-themed: near-white at the horizon lifting to the zenith
                // tint. The whole ramp lives low because the default framing
                // only exposes a few degrees of sky.
                let sky = mix(
                    self.sky_horizon.xyz,
                    self.sky_zenith.xyz,
                    smoothstep(0.0, 0.10, up)
                )
                // the ground below, hazing into the sky as it approaches the
                // horizon — without this the pale yellow meets the pink sky as
                // a hard saturated band
                let down = clamp(-t, 0.0, 1.0)
                let earth0 = mix(
                    self.ground_color.xyz,
                    self.ground_color.xyz * 0.88,
                    smoothstep(0.0, 0.75, down)
                )
                let ground_haze = 1.0 - smoothstep(0.006, 0.060, down)
                let earth = mix(earth0, self.sky_horizon.xyz, ground_haze)
                let glow = pow(1.0 - min(abs(t) * 26.0, 1.0), 3.0)
                let bg0 = mix(earth, sky, smoothstep(-0.010, 0.010, t))
                    + vec3(0.22, 0.20, 0.21) * glow * 0.07
                // the lamp: a small ~0.5 degree disc with a tight halo
                let sd = dot(dir, normalize(self.light_dir.xyz))
                let ang = acos(clamp(sd, -1.0, 1.0))
                let disc = 1.0 - smoothstep(0.0075, 0.0105, ang)
                let halo = (1.0 - smoothstep(0.0, 0.075, ang)) * 0.30
                let bg = bg0 + (vec3(1.0, 0.99, 0.94) * disc
                    + vec3(1.0, 0.88, 0.62) * halo) * self.light_dir.w
                return vec4(
                    scene.x + bg.x * (1.0 - scene.w),
                    scene.y + bg.y * (1.0 - scene.w),
                    scene.z + bg.z * (1.0 - scene.w),
                    1.0
                )
            }
        }
    }
}

#[derive(Script, ScriptHook, Debug)]
#[repr(C)]
pub struct DrawGridPlane {
    #[deref]
    pub draw_vars: DrawVars,
    /// grid line colour
    #[live(vec4(0.42, 0.41, 0.24, 1.0))]
    pub color: Vec4f,
    #[live(2.0)]
    pub extent: f32,
    #[live(0.05)]
    pub spacing: f32,
    /// ground fill colour (the grid lines use `color`)
    #[live(vec4(1.0, 1.0, 0.773, 1.0))]
    pub soil_color: Vec4f,
    /// second checker tone; equal to `soil_color` = no checker
    #[live(vec4(1.0, 1.0, 0.773, 1.0))]
    pub soil2_color: Vec4f,
    /// what the ground fades into at the horizon — must match the sky there,
    /// or the plane ends in a band of the wrong colour
    #[live(vec4(1.0, 0.985, 0.985, 1.0))]
    pub haze_color: Vec4f,
    #[live(0.0)]
    pub plane_y: f32,
    /// world size of one screen pixel per unit of distance:
    /// 2 * tan(fov_y / 2) / viewport_height_px
    #[live(0.001)]
    pub px_scale: f32,
    /// Light view-projection for the shadow lookup (see DrawRobotMesh).
    #[live]
    pub light_vp: Mat4f,
    /// x,y = shadow-map texel in UV; z = depth bias (w unused here).
    #[live(vec4(0.0005, 0.0005, 0.0015, 0.0))]
    pub shadow_texel: Vec4f,
    #[live(0.75)]
    pub shadow_strength: f32,
    /// 1 = flip V when sampling the shadow map (backend-dependent; Metal
    /// needs none, WebGL does).
    #[live(0.0)]
    pub shadow_flip: f32,
    /// >0.5 paints the ground with the raw shadow-map sample instead of the
    /// grid, so an empty map (uniform white) is distinguishable from a bad
    /// projection (map content in the wrong place). URDF_SHADOW_DEBUG=1.
    #[live(0.0)]
    pub debug_shadow: f32,
    #[live(1.0)]
    pub depth_clip: f32,
}

impl DrawGridPlane {
    pub fn draw(&mut self, cx: &mut CxDraw, geometry_id: GeometryId) {
        self.draw_vars.geometry_id = Some(geometry_id);
        if self.draw_vars.can_instance() {
            let new_area = cx.add_instance(&self.draw_vars);
            self.draw_vars.area = cx.update_area_refs(self.draw_vars.area, new_area);
        }
    }
}

#[derive(Script, ScriptHook, Debug)]
#[repr(C)]
pub struct DrawSceneComposite {
    #[deref]
    pub draw_super: DrawQuad,
    /// xyz = camera right, w = tan(fov_x / 2)
    #[live(vec4(1.0, 0.0, 0.0, 1.0))]
    pub cam_right: Vec4f,
    /// xyz = camera up, w = tan(fov_y / 2)
    #[live(vec4(0.0, 1.0, 0.0, 1.0))]
    pub cam_up: Vec4f,
    /// xyz = camera forward
    #[live(vec4(0.0, 0.0, -1.0, 0.0))]
    pub cam_fwd: Vec4f,
    /// xyz = direction towards the lamp, w = lamp switched on (0/1)
    #[live(vec4(-0.35, 0.84, 0.42, 0.0))]
    pub light_dir: Vec4f,
    /// sky colour at the horizon (also what the ground hazes into)
    #[live(vec4(1.0, 0.985, 0.985, 1.0))]
    pub sky_horizon: Vec4f,
    /// sky colour towards the zenith
    #[live(vec4(0.98, 0.895, 0.905, 1.0))]
    pub sky_zenith: Vec4f,
    /// ground colour below the horizon
    #[live(vec4(1.0, 1.0, 0.773, 1.0))]
    pub ground_color: Vec4f,
}

impl DrawSceneComposite {
    pub fn set_scene_texture(&mut self, texture: &Texture) {
        self.draw_super.draw_vars.set_texture(0, texture);
    }

}

#[derive(Script, ScriptHook, Debug)]
#[repr(C)]
pub struct DrawRobotMesh {
    #[deref]
    pub draw_vars: DrawVars,
    #[live]
    pub color: Vec4f,
    #[live]
    pub transform: Mat4f,
    #[live(vec3(1.0, 1.0, 1.0))]
    pub scale: Vec3f,
    /// xyz = direction towards the lamp, w = lamp switched on (0/1).
    /// Per-instance, not a uniform: set_uniform is unreliable on wasm.
    #[live(vec4(-0.35, 0.84, 0.42, 0.0))]
    pub key_light: Vec4f,
    /// Upper hemisphere ambient — set from the environment's sky colour so
    /// objects are lit by the world they are drawn in. Same per-instance
    /// reasoning as `key_light`.
    #[live(vec4(0.46, 0.45, 0.43, 1.0))]
    pub ambient_sky: Vec4f,
    /// Lower hemisphere ambient — the bounce off the ground plane.
    #[live(vec4(0.40, 0.39, 0.30, 1.0))]
    pub ambient_ground: Vec4f,
    /// Light view-projection for the shadow lookup. Per-instance rather than
    /// a uniform for the same reason as `key_light`.
    #[live]
    pub light_vp: Mat4f,
    /// x,y = one shadow-map texel in UV; z = constant depth bias;
    /// w = slope-scaled bias coefficient.
    #[live(vec4(0.0005, 0.0005, 0.0015, 0.006))]
    pub shadow_texel: Vec4f,
    /// How dark a fully occluded fragment goes (0 = shadows off).
    #[live(0.75)]
    pub shadow_strength: f32,
    /// 1 = flip V when sampling the shadow map (backend-dependent; Metal
    /// needs none, WebGL does).
    #[live(0.0)]
    pub shadow_flip: f32,
    #[live(1.0)]
    pub depth_clip: f32,
}

impl DrawRobotMesh {
    pub fn draw(&mut self, cx: &mut CxDraw, geometry_id: GeometryId) {
        self.draw_vars.geometry_id = Some(geometry_id);
        if self.draw_vars.can_instance() {
            let new_area = cx.add_instance(&self.draw_vars);
            self.draw_vars.area = cx.update_area_refs(self.draw_vars.area, new_area);
        }
    }
}


/// Depth-only draw for the shadow pass. Carries just enough to place the
/// geometry in the light's clip space — no material, no lighting.
#[derive(Script, ScriptHook, Debug)]
#[repr(C)]
pub struct DrawShadowDepth {
    #[deref]
    pub draw_vars: DrawVars,
    #[live]
    pub transform: Mat4f,
    #[live(vec3(1.0, 1.0, 1.0))]
    pub scale: Vec3f,
    #[live]
    pub light_vp: Mat4f,
}

impl DrawShadowDepth {
    pub fn draw(&mut self, cx: &mut CxDraw, geometry_id: GeometryId) {
        self.draw_vars.geometry_id = Some(geometry_id);
        if self.draw_vars.can_instance() {
            let new_area = cx.add_instance(&self.draw_vars);
            self.draw_vars.area = cx.update_area_refs(self.draw_vars.area, new_area);
        }
    }
}


/// Instance data for the planar-reflection draw. `transform` carries the
/// reflection matrix folded with the link transform.
#[derive(Script, ScriptHook, Debug)]
#[repr(C)]
pub struct DrawRobotReflect {
    #[deref]
    pub draw_vars: DrawVars,
    #[live]
    pub color: Vec4f,
    #[live]
    pub transform: Mat4f,
    #[live(vec3(1.0, 1.0, 1.0))]
    pub scale: Vec3f,
    #[live(vec4(-0.35, 0.84, 0.42, 0.0))]
    pub key_light: Vec4f,
    #[live(vec4(0.46, 0.45, 0.43, 1.0))]
    pub ambient_sky: Vec4f,
    #[live(vec4(0.40, 0.39, 0.30, 1.0))]
    pub ambient_ground: Vec4f,
    /// Blend weight of the mirror image — MuJoCo's material reflectance.
    #[live(0.2)]
    pub reflect_alpha: f32,
}

impl DrawRobotReflect {
    pub fn draw(&mut self, cx: &mut CxDraw, geometry_id: GeometryId) {
        self.draw_vars.geometry_id = Some(geometry_id);
        if self.draw_vars.can_instance() {
            let new_area = cx.add_instance(&self.draw_vars);
            self.draw_vars.area = cx.update_area_refs(self.draw_vars.area, new_area);
        }
    }
}
