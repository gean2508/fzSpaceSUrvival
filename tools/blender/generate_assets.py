# =============================================================================
# GENERATE_ASSETS.PY — Script de generación de assets para Agencia Espacial Survival
# -----------------------------------------------------------------------------
# Este script es una HERRAMIENTA EXTERNA al juego Rust/Bevy.
# Corre DENTRO de Blender (Scripting tab > Run Script) o headless:
#   blender --background --python tools/blender/generate_assets.py
# NO es parte del runtime del juego. Se ejecuta bajo demanda para generar
# los archivos .glb que Bevy carga como assets estáticos opcionales.
# Ubicación: tools/blender/ (fuera del crate ha-game).
# Salida juego: native/crates/ha-game/assets/models/{catalog_kind}.glb
# Staging (kinds futuros): tools/blender/staging/{name}.glb
# =============================================================================

from __future__ import annotations

import math
import os
import random
import sys
import traceback

import bpy
import bmesh
from mathutils import Matrix, Vector

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
PROJECT_ROOT = os.path.normpath(os.path.join(SCRIPT_DIR, "..", ".."))
OUTPUT_DIR = os.path.join(PROJECT_ROOT, "native", "crates", "ha-game", "assets", "models")
STAGING_DIR = os.path.join(SCRIPT_DIR, "staging")
MAX_TRIS_WARN = 350

# --- Paleta (hex sRGB) -------------------------------------------------------

MADERA_CLARA = "#c4956a"
MADERA_OSCURA = "#7a4f2d"
MADERA_VETA = "#5c3520"
METAL_OXIDADO = "#8b5e3c"
METAL_GRIS = "#6b7280"
METAL_OSCURO = "#374151"
METAL_COBRE = "#b87333"
NIEVE = "#e8eef4"
VERDE_BOSQUE = "#3d6b2a"
VERDE_CLARO = "#5a8a3a"
PIEDRA = "#9ca3af"
PIEDRA_OSCURA = "#6b7280"
LUNA_GRIS = "#c8cdd4"
LUNA_OSCURO = "#8a8f96"
NARANJA_ACENTO = "#e67e22"
AMARILLO_ACENTO = "#f4d03f"
AZUL_PANEL = "#1e3a8a"
ROJO_ACENTO = "#c0392b"
BLANCO_NIEVE = "#f0f4f8"
TRONCO_NIEVE = "#d4c5a9"


def hex_to_linear(hex_color: str):
    hex_color = hex_color.lstrip("#")
    r, g, b = [int(hex_color[i : i + 2], 16) / 255.0 for i in (0, 2, 4)]

    def to_linear(c):
        return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4

    return (to_linear(r), to_linear(g), to_linear(b), 1.0)


def make_material(name: str, hex_color: str, roughness=0.85, metallic=0.0):
    if name in bpy.data.materials:
        return bpy.data.materials[name]
    mat = bpy.data.materials.new(name=name)
    mat.use_nodes = True
    bsdf = mat.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = hex_to_linear(hex_color)
    bsdf.inputs["Roughness"].default_value = roughness
    bsdf.inputs["Metallic"].default_value = metallic
    return mat


def assign_material(obj, mat):
    if obj.type != "MESH":
        return
    obj.data.materials.clear()
    obj.data.materials.append(mat)


def clear_scene():
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.object.delete()
    for block in list(bpy.data.meshes):
        if block.users == 0:
            bpy.data.meshes.remove(block)
    for block in list(bpy.data.materials):
        if block.users == 0:
            bpy.data.materials.remove(block)


def link_object(obj):
    col = bpy.context.scene.collection
    if obj.name not in col.objects:
        col.objects.link(obj)


def iter_hierarchy(obj):
    yield obj
    for child in obj.children:
        yield from iter_hierarchy(child)


def mesh_objects(root):
    return [o for o in iter_hierarchy(root) if o.type == "MESH"]


def parent_keep_transform(child, parent):
    child.parent = parent
    child.matrix_parent_inverse = parent.matrix_world.inverted()


def create_empty(name: str):
    obj = bpy.data.objects.new(name, None)
    link_object(obj)
    return obj


def _bm_to_object(name: str, bm: bmesh.types.BMesh) -> bpy.types.Object:
    mesh = bpy.data.meshes.new(name + "_mesh")
    bm.to_mesh(mesh)
    bm.free()
    obj = bpy.data.objects.new(name, mesh)
    link_object(obj)
    return obj


def make_cylinder(
    name,
    radius,
    depth,
    segments=8,
    location=(0, 0, 0),
    rotation=(0, 0, 0),
    mat=None,
):
    bm = bmesh.new()
    bmesh.ops.create_cone(
        bm,
        cap_ends=True,
        cap_tris=False,
        segments=segments,
        radius1=radius,
        radius2=radius,
        depth=depth,
    )
    obj = _bm_to_object(name, bm)
    obj.location = Vector(location)
    obj.rotation_euler = rotation
    if mat:
        assign_material(obj, mat)
    return obj


def make_cube(name, scale=(1, 1, 1), location=(0, 0, 0), rotation=(0, 0, 0), mat=None):
    bm = bmesh.new()
    bmesh.ops.create_cube(bm, size=1.0)
    obj = _bm_to_object(name, bm)
    obj.scale = Vector(scale)
    obj.location = Vector(location)
    obj.rotation_euler = rotation
    if mat:
        assign_material(obj, mat)
    return obj


def make_icosphere(name, radius, subdiv=1, location=(0, 0, 0), scale=(1, 1, 1), mat=None):
    bm = bmesh.new()
    bmesh.ops.create_icosphere(bm, subdivisions=subdiv, radius=radius)
    obj = _bm_to_object(name, bm)
    obj.location = Vector(location)
    obj.scale = Vector(scale)
    if mat:
        assign_material(obj, mat)
    return obj


def make_cone(
    name,
    radius1,
    radius2,
    depth,
    segments=8,
    location=(0, 0, 0),
    rotation=(0, 0, 0),
    mat=None,
):
    bm = bmesh.new()
    bmesh.ops.create_cone(
        bm,
        cap_ends=True,
        cap_tris=False,
        segments=segments,
        radius1=radius1,
        radius2=radius2,
        depth=depth,
    )
    obj = _bm_to_object(name, bm)
    obj.location = Vector(location)
    obj.rotation_euler = rotation
    if mat:
        assign_material(obj, mat)
    return obj


def make_uv_sphere(name, radius, segments=8, ring_count=4, location=(0, 0, 0), mat=None):
    bm = bmesh.new()
    bmesh.ops.create_uvsphere(bm, u_segments=segments, v_segments=ring_count, radius=radius)
    obj = _bm_to_object(name, bm)
    obj.location = Vector(location)
    if mat:
        assign_material(obj, mat)
    return obj


def make_torus(
    name,
    major_radius,
    minor_radius,
    major_segments=12,
    minor_segments=6,
    location=(0, 0, 0),
    mat=None,
):
    bpy.ops.mesh.primitive_torus_add(
        major_radius=major_radius,
        minor_radius=minor_radius,
        major_segments=major_segments,
        minor_segments=minor_segments,
        location=location,
    )
    obj = bpy.context.active_object
    obj.name = name
    if mat:
        assign_material(obj, mat)
    return obj


def make_plane(name, size=1.0, location=(0, 0, 0), rotation=(0, 0, 0), mat=None):
    bm = bmesh.new()
    bmesh.ops.create_grid(bm, x_segments=1, y_segments=1, size=size)
    obj = _bm_to_object(name, bm)
    obj.location = Vector(location)
    obj.rotation_euler = rotation
    if mat:
        assign_material(obj, mat)
    return obj


def displace_verts(obj, strength=0.2, seed=42):
    if obj.type != "MESH":
        return
    random.seed(seed)
    me = obj.data
    for v in me.vertices:
        v.co.x += random.uniform(-strength, strength)
        v.co.y += random.uniform(-strength, strength)
        v.co.z += random.uniform(-strength * 0.5, strength * 0.5)
    me.update()


def clip_mesh_above_z(obj, z_cut):
    if obj.type != "MESH":
        return
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    to_del = [v for v in bm.verts if (obj.matrix_world @ v.co).z < z_cut]
    if to_del:
        bmesh.ops.delete(bm, geom=to_del, context="VERTS")
    bm.to_mesh(obj.data)
    bm.free()
    obj.data.update()


def join_objects(objects, name):
    meshes = [o for o in objects if o.type == "MESH"]
    if not meshes:
        return objects[0] if objects else None
    if len(meshes) == 1:
        meshes[0].name = name
        return meshes[0]
    bpy.ops.object.select_all(action="DESELECT")
    for o in meshes:
        o.select_set(True)
    bpy.context.view_layer.objects.active = meshes[0]
    bpy.ops.object.join()
    result = bpy.context.view_layer.objects.active
    result.name = name
    return result


def apply_all_transforms(root):
    for obj in mesh_objects(root):
        bpy.ops.object.select_all(action="DESELECT")
        obj.select_set(True)
        bpy.context.view_layer.objects.active = obj
        bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)


def set_origin_to_base(root):
    apply_all_transforms(root)
    min_z = math.inf
    for obj in mesh_objects(root):
        for corner in obj.bound_box:
            w = obj.matrix_world @ Vector(corner)
            min_z = min(min_z, w.z)
    if not math.isfinite(min_z):
        return
    shift = Vector((0, 0, -min_z))
    for obj in iter_hierarchy(root):
        obj.matrix_world.translation += shift


def count_tris(root) -> int:
    total = 0
    for obj in mesh_objects(root):
        me = obj.data
        for poly in me.polygons:
            total += max(0, len(poly.vertices) - 2)
    return total


def export_asset(root_obj, kind: str, output_dir: str) -> dict:
    os.makedirs(output_dir, exist_ok=True)
    output_path = os.path.join(output_dir, f"{kind}.glb")

    def select_hierarchy(obj):
        obj.select_set(True)
        for child in obj.children:
            select_hierarchy(child)

    bpy.ops.object.select_all(action="DESELECT")
    select_hierarchy(root_obj)
    bpy.context.view_layer.objects.active = root_obj

    total_tris = count_tris(root_obj)

    bpy.ops.export_scene.gltf(
        filepath=output_path,
        export_format="GLB",
        export_apply=True,
        export_materials="EXPORT",
        export_cameras=False,
        export_lights=False,
        use_selection=True,
        export_extras=True,
        export_yup=True,
        export_tangents=False,
        export_colors=False,
    )

    return {"kind": kind, "path": output_path, "triangles": total_tris}


# --- Asset builders ----------------------------------------------------------


def make_tree_oak():
    mat_trunk = make_material("mat_trunk_oak", MADERA_OSCURA)
    mat_snow = make_material("mat_snow", NIEVE)
    mat_leaf = make_material("mat_leaf_oak", VERDE_BOSQUE)

    trunk = make_cylinder("trunk", 0.35, 3.0, segments=8, location=(0, 0, 1.5), mat=mat_trunk)
    displace_verts(trunk, 0.05, 11)

    snow_ring = make_cylinder(
        "snow_ring", 0.38, 0.04, segments=8, location=(0, 0, 1.5), mat=mat_snow
    )

    canopy = create_empty("canopy")
    parent_keep_transform(canopy, trunk)
    canopy.location = (0, 0, 3.0)

    c1 = make_icosphere("c1", 2.0, subdiv=1, location=(0, 0, 0.5), scale=(1, 1, 0.8), mat=mat_leaf)
    snow1 = make_icosphere("snow1", 2.05, subdiv=1, location=(0, 0, 1.5), mat=mat_snow)
    clip_mesh_above_z(snow1, trunk.location.z + 4.5)

    c2 = make_icosphere("c2", 1.4, subdiv=1, location=(0, 0, 2.0), mat=mat_leaf)
    snow2 = make_icosphere("snow2", 1.45, subdiv=1, location=(0, 0, 2.5), mat=mat_snow)
    clip_mesh_above_z(snow2, trunk.location.z + 5.5)

    c3 = make_cone("c3", 0.6, 0.0, 1.0, segments=8, location=(0, 0, 3.2), mat=mat_leaf)

    for child in (c1, snow1, c2, snow2, c3):
        parent_keep_transform(child, canopy)

    return trunk


def make_tree_birch():
    mat_trunk = make_material("mat_birch", TRONCO_NIEVE)
    mat_mark = make_material("mat_birch_mark", MADERA_OSCURA)
    mat_leaf = make_material("mat_birch_leaf", VERDE_CLARO)
    mat_snow = make_material("mat_snow_birch", NIEVE)

    trunk = make_cylinder("trunk", 0.18, 4.5, segments=6, location=(0, 0, 2.25), mat=mat_trunk)
    for i, z in enumerate([1.2, 2.2, 3.2]):
        mark = make_cube(
            f"mark_{i}",
            scale=(0.2, 0.05, 0.4),
            location=(0.2, 0, z),
            mat=mat_mark,
        )
        parent_keep_transform(mark, trunk)

    canopy = create_empty("canopy")
    parent_keep_transform(canopy, trunk)
    canopy.location = (0, 0, 4.5)

    crown = make_icosphere("crown", 1.5, subdiv=1, location=(0, 0, 0.5), scale=(0.8, 0.8, 1.1), mat=mat_leaf)
    snow = make_icosphere("snow_cap", 1.55, subdiv=1, location=(0, 0, 1.3), mat=mat_snow)
    clip_mesh_above_z(snow, trunk.location.z + 5.8)
    parent_keep_transform(crown, canopy)
    parent_keep_transform(snow, canopy)

    for i, ang in enumerate([30, 45, -20]):
        rad = math.radians(ang)
        branch = make_cylinder(
            f"branch_{i}",
            0.05,
            1.2,
            segments=6,
            location=(math.sin(rad) * 0.4, math.cos(rad) * 0.4, -1.2),
            rotation=(0, ang, math.pi / 2),
            mat=mat_trunk,
        )
        parent_keep_transform(branch, canopy)

    return trunk


def make_rock_medium():
    mat_stone = make_material("mat_rock_med", PIEDRA)
    mat_snow = make_material("mat_snow_rock", NIEVE)
    body = make_icosphere("rock", 1.0, subdiv=1, location=(0, 0, 0.65), scale=(1, 1, 0.65), mat=mat_stone)
    displace_verts(body, 0.25, 42)
    snow = make_cylinder("snow_cap", 0.9, 0.08, segments=8, location=(0, 0, 1.05), mat=mat_snow)
    displace_verts(snow, 0.1, 3)
    return join_objects([body, snow], "rock_medium")


def make_rock_large():
    mat_stone = make_material("mat_rock_large", PIEDRA_OSCURA)
    mat_snow = make_material("mat_snow_rock_l", NIEVE)
    body = make_icosphere("rock", 1.8, subdiv=1, location=(0, 0, 1.2), scale=(1, 0.85, 0.7), mat=mat_stone)
    displace_verts(body, 0.4, 7)
    snow = make_cylinder("snow_cap", 1.5, 0.1, segments=8, location=(0, 0, 1.85), mat=mat_snow)
    return join_objects([body, snow], "rock_large")


def make_shrub_bush():
    mat_leaf = make_material("mat_shrub", VERDE_BOSQUE)
    mat_snow = make_material("mat_shrub_snow", NIEVE)
    parts = []
    for i, (pos, r) in enumerate([((0, 0, 0.45), 0.55), ((0.4, 0.2, 0.4), 0.65), ((-0.3, 0.3, 0.38), 0.5)]):
        p = make_icosphere(f"b{i}", r, subdiv=1, location=pos, scale=(1, 1, 0.8), mat=mat_leaf)
        s = make_cylinder(f"s{i}", r * 0.9, 0.05, segments=6, location=(pos[0], pos[1], pos[2] + r * 0.7), mat=mat_snow)
        parts.extend([p, s])
    return join_objects(parts, "shrub_bush")


def make_grass_tuft():
    mat_grass = make_material("mat_grass", VERDE_CLARO)
    mat_base = make_material("mat_grass_base", PIEDRA_OSCURA)
    base = make_cylinder("base", 0.15, 0.03, segments=6, location=(0, 0, 0.015), mat=mat_base)
    blades = []
    for i in range(6):
        ang = math.radians(i * 30)
        blade = make_plane(f"blade_{i}", size=0.55, location=(0, 0, 0.28), mat=mat_grass)
        blade.scale = (0.06, 0.55, 1)
        blade.rotation_euler = (math.radians(15), 0, ang)
        blades.append(blade)
    return join_objects([base] + blades, "grass_tuft")


def make_wood_log():
    mat_wood = make_material("mat_log", MADERA_CLARA)
    mat_end = make_material("mat_log_end", MADERA_OSCURA)
    mat_snow = make_material("mat_log_snow", NIEVE)
    log = make_cylinder("log", 0.28, 3.0, segments=8, location=(0, 0, 0.28), rotation=(0, math.pi / 2, 0), mat=mat_wood)
    displace_verts(log, 0.03, 5)
    end_l = make_cylinder("end_l", 0.28, 0.05, segments=8, location=(-1.5, 0, 0.28), rotation=(0, math.pi / 2, 0), mat=mat_end)
    end_r = make_cylinder("end_r", 0.28, 0.05, segments=8, location=(1.5, 0, 0.28), rotation=(0, math.pi / 2, 0), mat=mat_end)
    snow = make_cube("snow", scale=(3.0, 0.3, 0.05), location=(0, 0, 0.58), mat=mat_snow)
    return join_objects([log, end_l, end_r, snow], "wood_log")


def make_campfire():
    mat_stone = make_material("mat_cf_stone", PIEDRA)
    mat_wood = make_material("mat_cf_wood", MADERA_OSCURA)
    mat_ember = make_material("mat_cf_ember", ROJO_ACENTO, roughness=0.95)
    mat_flame = make_material("mat_cf_flame", NARANJA_ACENTO, roughness=0.3)

    root = create_empty("campfire_root")
    for i in range(8):
        ang = i * (math.pi * 2 / 8)
        stone = make_cylinder(
            f"stone_{i}",
            0.12,
            0.1,
            segments=6,
            location=(math.cos(ang) * 0.6, math.sin(ang) * 0.6, 0.05),
            mat=mat_stone,
        )
        stone.scale.z = 0.4
        parent_keep_transform(stone, root)

    for i, rot in enumerate([0, math.pi / 2, math.pi / 4, -math.pi / 4]):
        log = make_cylinder(f"log_{i}", 0.08, 0.7, segments=6, location=(0, 0, 0.12), rotation=(rot, 0, math.pi / 2), mat=mat_wood)
        parent_keep_transform(log, root)

    ember = make_cylinder("ember", 0.3, 0.04, segments=8, location=(0, 0, 0.08), mat=mat_ember)
    parent_keep_transform(ember, root)

    flame = make_cone("flame", 0.15, 0.0, 0.6, segments=6, location=(0, 0, 0.35), mat=mat_flame)
    parent_keep_transform(flame, root)

    return root


def make_flower(color_hex: str, name: str):
    mat_stem = make_material(f"mat_stem_{name}", VERDE_CLARO)
    mat_petal = make_material(f"mat_petal_{name}", color_hex)
    mat_center = make_material(f"mat_center_{name}", AMARILLO_ACENTO)

    stem = make_cylinder("stem", 0.015, 0.28, segments=4, location=(0, 0, 0.14), mat=mat_stem)
    petals = []
    for i in range(5):
        ang = math.radians(i * 72)
        petal = make_plane(f"petal_{i}", size=0.1, location=(0, 0, 0.28), mat=mat_petal)
        petal.scale = (0.07, 0.1, 1)
        petal.rotation_euler = (math.radians(25), 0, ang)
        petals.append(petal)
    center = make_uv_sphere("center", 0.025, segments=4, ring_count=2, location=(0, 0, 0.3), mat=mat_center)
    return join_objects([stem] + petals + [center], name)


def make_stone_pillar():
    mat_stone = make_material("mat_pillar", PIEDRA)
    mat_dark = make_material("mat_pillar_dark", PIEDRA_OSCURA)
    mat_snow = make_material("mat_pillar_snow", NIEVE)
    body = make_cylinder("body", 0.38, 4.0, segments=8, location=(0, 0, 2.1), mat=mat_stone)
    displace_verts(body, 0.04, 15)
    base = make_cylinder("base", 0.48, 0.2, segments=8, location=(0, 0, 0.1), mat=mat_dark)
    cap = make_cylinder("cap", 0.48, 0.15, segments=8, location=(0, 0, 4.18), mat=mat_dark)
    snow = make_cylinder("snow", 0.5, 0.06, segments=8, location=(0, 0, 4.28), mat=mat_snow)
    return join_objects([base, body, cap, snow], "stone_pillar")


def make_chest_wood():
    mat_light = make_material("mat_chest_light", MADERA_CLARA)
    mat_dark = make_material("mat_chest_dark", MADERA_OSCURA)
    mat_metal = make_material("mat_chest_metal", METAL_GRIS)

    body = make_cube("body", scale=(1.0, 0.7, 0.7), location=(0, 0, 0.35), mat=mat_light)
    door = make_cube("door", scale=(1.0, 0.25, 0.7), location=(0, 0, 0.83), mat=mat_dark)
    parent_keep_transform(door, body)

    for x in (-0.35, 0.35):
        hinge = make_cylinder("hinge", 0.04, 0.15, segments=4, location=(x, 0, 0.72), rotation=(math.pi / 2, 0, 0), mat=mat_metal)
        parent_keep_transform(hinge, body)
    latch = make_cube("latch", scale=(0.2, 0.05, 0.1), location=(0, 0.38, 0.83), mat=mat_metal)
    parent_keep_transform(latch, body)
    return body


def make_cabin_small():
    mat_wall = make_material("mat_cabin_wall", MADERA_CLARA)
    mat_roof = make_material("mat_cabin_roof", MADERA_OSCURA)
    mat_snow = make_material("mat_cabin_snow", NIEVE)
    mat_dark = make_material("mat_cabin_dark", MADERA_OSCURA)
    mat_stone = make_material("mat_cabin_stone", PIEDRA_OSCURA)

    body = make_cube("body", scale=(4.0, 3.0, 4.0), location=(0, 0, 1.5), mat=mat_wall)
    roof = make_cube("roof", scale=(4.5, 1.8, 4.2), location=(0, 0, 3.9), mat=mat_roof)
    roof.scale.y = 0.5
    snow_roof = make_cube("snow_roof", scale=(4.6, 0.2, 4.3), location=(0, 0, 4.55), mat=mat_snow)
    door = make_cube("door", scale=(0.9, 1.8, 0.15), location=(0, 0, 0.9), mat=mat_dark)
    parent_keep_transform(door, body)
    for x in (-1.2, 1.2):
        win = make_cube("window", scale=(0.6, 0.6, 0.12), location=(x, 0, 1.8), mat=mat_dark)
        parent_keep_transform(win, body)
    chimney = make_cube("chimney", scale=(0.4, 1.0, 0.4), location=(1.2, 0, 4.3), mat=mat_stone)
    step = make_cube("step", scale=(0.8, 0.15, 0.4), location=(0, -2.1, 0.075), mat=mat_stone)
    return join_objects([body, roof, snow_roof, chimney, step], "cabin_small")


def make_machine_wood_extractor():
    mat_wood = make_material("mat_mwe_wood", MADERA_OSCURA)
    mat_metal = make_material("mat_mwe_metal", METAL_GRIS)
    mat_rust = make_material("mat_mwe_rust", METAL_OXIDADO)

    base = make_cube("base", scale=(2.0, 0.4, 2.0), location=(0, 0, 0.2), mat=mat_wood)
    tower = make_cube("tower", scale=(0.4, 2.5, 0.4), location=(0, 0, 1.65), mat=mat_wood)
    parent_keep_transform(tower, base)

    arm = make_cube("arm", scale=(0.3, 0.3, 2.0), location=(0, 0, 1.0), mat=mat_wood)
    parent_keep_transform(arm, tower)

    pulley = make_torus("pulley", 0.15, 0.04, major_segments=8, minor_segments=4, location=(0, 0, 1.0), mat=mat_metal)
    parent_keep_transform(pulley, arm)

    rope = make_cylinder("rope", 0.02, 1.5, segments=4, location=(0, 0, -0.75), mat=mat_wood)
    parent_keep_transform(rope, arm)

    rotor = make_cylinder("rotor", 0.4, 0.08, segments=8, location=(0, 0, 0.44), mat=mat_rust)
    parent_keep_transform(rotor, base)

    for i, z in enumerate([1.0, 1.5, 2.0, 2.5, 2.8, 3.0]):
        rivet = make_uv_sphere(f"rivet_{i}", 0.03, segments=4, ring_count=2, location=(0.22, 0, z - 1.65), mat=mat_metal)
        parent_keep_transform(rivet, tower)

    return base


def make_machine_stone_drill():
    mat_metal = make_material("mat_msd_metal", METAL_GRIS)
    mat_rust = make_material("mat_msd_rust", METAL_OXIDADO)
    mat_dark = make_material("mat_msd_dark", METAL_OSCURO)
    mat_red = make_material("mat_msd_red", ROJO_ACENTO)

    base = make_cube("base", scale=(1.8, 0.5, 1.8), location=(0, 0, 0.25), mat=mat_metal)
    body = make_cylinder("body", 0.6, 1.5, segments=8, location=(0, 0, 1.25), mat=mat_rust)
    stack = make_cylinder("stack", 0.12, 0.8, segments=4, location=(0.5, 0, 1.8), mat=mat_dark)
    rotor = make_cone("rotor", 0.2, 0.0, 0.8, segments=8, location=(0, 0, -0.2), rotation=(math.pi, 0, 0), mat=mat_metal)
    parent_keep_transform(rotor, body)
    gear = make_cylinder("gear", 0.5, 0.08, segments=12, location=(0.7, 0, 1.0), mat=mat_dark)
    gauge = make_cylinder("gauge", 0.15, 0.03, segments=8, location=(-0.55, 0, 1.5), mat=mat_metal)
    needle = make_cylinder("needle", 0.01, 0.12, segments=4, location=(-0.55, 0, 1.52), rotation=(0, math.pi / 4, 0), mat=mat_red)
    return join_objects([base, body, stack, gear, gauge, needle], "machine_stone_drill")


def make_machine_pump():
    mat_metal = make_material("mat_pump_metal", METAL_GRIS)
    mat_rust = make_material("mat_pump_rust", METAL_OXIDADO)

    base = make_cube("base", scale=(1.0, 0.3, 1.0), location=(0, 0, 0.15), mat=mat_metal)
    body = make_cylinder("body", 0.4, 1.2, segments=8, location=(0, 0, 0.9), mat=mat_rust)
    head = make_uv_sphere("head", 0.42, segments=8, ring_count=4, location=(0, 0, 1.55), mat=mat_rust)
    arm = make_cube("arm", scale=(0.12, 0.8, 0.12), location=(0, 0, 0.5), mat=mat_metal)
    parent_keep_transform(arm, body)
    pipe = make_cylinder("pipe", 0.08, 0.8, segments=4, location=(0.5, 0, 0.2), rotation=(0, 0, math.pi / 2), mat=mat_metal)
    return join_objects([base, body, head, pipe], "machine_pump")


def make_habitation_dome():
    mat_metal = make_material("mat_dome_metal", METAL_GRIS, roughness=0.4, metallic=0.3)
    mat_dark = make_material("mat_dome_dark", METAL_OSCURO)
    mat_glass = make_material("mat_dome_glass", AZUL_PANEL, roughness=0.1)

    base = make_cylinder("base", 2.5, 0.6, segments=16, location=(0, 0, 0.3), mat=mat_metal)
    dome = make_icosphere("dome", 2.5, subdiv=1, location=(0, 0, 1.8), scale=(1, 1, 0.55), mat=mat_metal)
    clip_mesh_above_z(dome, 1.25)
    window = make_cylinder("window", 0.5, 0.05, segments=12, location=(0, 2.45, 1.5), rotation=(math.pi / 2, 0, 0), mat=mat_glass)
    door = make_cube("door", scale=(0.8, 1.5, 0.15), location=(0, 2.5, 0.75), mat=mat_dark)
    parent_keep_transform(door, base)
    tube = make_cylinder("tube", 0.5, 1.0, segments=8, location=(0, 3.2, 0.5), rotation=(0, 0, math.pi / 2), mat=mat_metal)
    return join_objects([base, dome, window, tube], "habitation_dome")


def make_solar_panel():
    mat_metal = make_material("mat_solar_metal", METAL_GRIS)
    mat_dark = make_material("mat_solar_dark", METAL_OSCURO)
    mat_panel = make_material("mat_solar_panel", AZUL_PANEL, roughness=0.05, metallic=0.1)

    pole = make_cylinder("pole", 0.06, 1.2, segments=6, location=(0, 0, 0.6), mat=mat_metal)
    joint = make_uv_sphere("joint", 0.09, segments=6, ring_count=3, location=(0, 0, 1.2), mat=mat_dark)
    panel = make_cube("panel", scale=(1.2, 0.06, 2.0), location=(0, 0, 1.35), rotation=(math.radians(15), 0, 0), mat=mat_panel)
    parent_keep_transform(panel, pole)
    return pole


def make_storage_box_small():
    mat_metal = make_material("mat_box_metal", METAL_GRIS)
    mat_dark = make_material("mat_box_dark", METAL_OSCURO)
    mat_rust = make_material("mat_box_rust", METAL_OXIDADO)
    mat_accent = make_material("mat_box_accent", NARANJA_ACENTO)

    body = make_cube("body", scale=(1.0, 0.9, 1.0), location=(0, 0, 0.45), mat=mat_metal)
    lid = make_cube("lid", scale=(1.0, 0.15, 1.0), location=(0, 0, 0.98), mat=mat_dark)
    cross_a = make_cube("cross_a", scale=(0.9, 0.04, 0.04), location=(0, 0, 0.55), mat=mat_rust)
    cross_b = make_cube("cross_b", scale=(0.04, 0.04, 0.9), location=(0, 0, 0.55), mat=mat_rust)
    lock = make_cube("lock", scale=(0.12, 0.05, 0.08), location=(0, 0.48, 0.55), mat=mat_accent)
    return join_objects([body, lid, cross_a, cross_b, lock], "storage_box_small")


def make_launch_pad():
    mat_metal = make_material("mat_pad_metal", METAL_GRIS, roughness=0.6, metallic=0.4)
    mat_dark = make_material("mat_pad_dark", METAL_OSCURO)
    mat_orange = make_material("mat_pad_orange", NARANJA_ACENTO)
    mat_yellow = make_material("mat_pad_yellow", AMARILLO_ACENTO)

    platform = make_cube("platform", scale=(4.0, 0.4, 4.0), location=(0, 0, 0.2), mat=mat_metal)
    parts = [platform]
    for x, z in [(-1.8, -1.8), (1.8, -1.8), (-1.8, 1.8), (1.8, 1.8)]:
        leg = make_cylinder("leg", 0.25, 0.8, segments=8, location=(x, 0, -0.2), mat=mat_dark)
        parts.append(leg)
    for x, z in [(-0.5, -0.5), (0.5, -0.5), (-0.5, 0.5), (0.5, 0.5)]:
        guide = make_cylinder("guide", 0.1, 1.5, segments=6, location=(x, 0, 0.95), mat=mat_orange)
        parts.append(guide)
    stripe = make_cube("stripe", scale=(3.8, 0.05, 0.2), location=(0, 0, 1.95), mat=mat_yellow)
    parts.append(stripe)
    return join_objects(parts, "launch_pad")


def make_antenna_simple():
    mat_metal = make_material("mat_ant_metal", METAL_GRIS)
    base = make_cube("base", scale=(0.6, 0.3, 0.6), location=(0, 0, 0.15), mat=mat_metal)
    mast = make_cylinder("mast", 0.05, 3.0, segments=4, location=(0, 0, 1.8), mat=mat_metal)
    for i, (y, length) in enumerate([(1.0, 1.0), (2.0, 0.7), (2.8, 0.4)]):
        bar = make_cylinder(f"bar_{i}", 0.03, length, segments=4, location=(length / 2, 0, y), rotation=(0, 0, math.pi / 2), mat=mat_metal)
        parent_keep_transform(bar, mast)
    dish = create_empty("dish")
    tip = make_cone("tip", 0.04, 0.0, 0.2, segments=4, location=(0, 0, 3.1), mat=mat_metal)
    parent_keep_transform(dish, mast)
    parent_keep_transform(tip, dish)
    parent_keep_transform(mast, base)
    return base


def make_fuel_tank_tall():
    mat_yellow = make_material("mat_tank_yellow", AMARILLO_ACENTO)
    mat_orange = make_material("mat_tank_orange", NARANJA_ACENTO)
    mat_metal = make_material("mat_tank_metal", METAL_GRIS)

    body = make_cylinder("body", 0.55, 3.0, segments=8, location=(0, 0, 1.5), mat=mat_yellow)
    top = make_uv_sphere("top", 0.55, segments=8, ring_count=4, location=(0, 0, 3.05), mat=mat_yellow)
    bottom = make_uv_sphere("bottom", 0.55, segments=8, ring_count=4, location=(0, 0, -0.05), mat=mat_yellow)
    for z in (1.0, 2.0):
        band = make_cylinder(f"band_{z}", 0.57, 0.12, segments=8, location=(0, 0, z), mat=mat_orange)
        parent_keep_transform(band, body)
    pipe = make_cylinder("pipe", 0.08, 0.6, segments=4, location=(0.6, 0, 0.2), rotation=(0, 0, math.pi / 2), mat=mat_metal)
    gauge = make_cube("gauge", scale=(0.08, 2.0, 0.04), location=(0.56, 0, 1.5), mat=make_material("mat_tank_dark", METAL_OSCURO))
    return join_objects([body, top, bottom, pipe, gauge], "fuel_tank_tall")


def make_rocket_cabin():
    mat_copper = make_material("mat_rocket_copper", METAL_COBRE)
    mat_glass = make_material("mat_rocket_glass", AZUL_PANEL, roughness=0.05)
    mat_rust = make_material("mat_rocket_rust", METAL_OXIDADO)
    mat_dark = make_material("mat_rocket_dark", METAL_OSCURO)

    body = make_cylinder("body", 0.8, 2.0, segments=8, location=(0, 0, 1.0), mat=mat_copper)
    nose = make_cone("nose", 0.8, 0.0, 0.8, segments=8, location=(0, 0, 2.4), mat=mat_copper)
    for i, ang in enumerate([0, 2.1, 4.2]):
        w = make_cylinder(f"win_{i}", 0.2, 0.05, segments=8, location=(math.cos(ang) * 0.8, math.sin(ang) * 0.8, 1.2), rotation=(0, ang, math.pi / 2), mat=mat_glass)
        parent_keep_transform(w, body)
    band = make_cylinder("band", 0.82, 0.15, segments=8, location=(0, 0, 0.2), mat=mat_rust)
    return join_objects([body, nose, band], "rocket_module_cabin")


def make_rocket_fuel():
    mat_copper = make_material("mat_rf_copper", METAL_COBRE)
    mat_rust = make_material("mat_rf_rust", METAL_OXIDADO)
    mat_metal = make_material("mat_rf_metal", METAL_GRIS)
    body = make_cylinder("body", 0.8, 2.5, segments=8, location=(0, 0, 1.25), mat=mat_copper)
    for z in (0.4, 2.1):
        band = make_cylinder(f"band_{z}", 0.82, 0.12, segments=8, location=(0, 0, z), mat=mat_rust)
        parent_keep_transform(band, body)
    valve = make_cube("valve", scale=(0.15, 0.3, 0.15), location=(0.85, 0, 1.5), mat=mat_metal)
    return join_objects([body, valve], "rocket_module_fuel")


def make_rocket_engine():
    mat_copper = make_material("mat_re_copper", METAL_COBRE)
    mat_metal = make_material("mat_re_metal", METAL_GRIS)
    mat_dark = make_material("mat_re_dark", METAL_OSCURO)
    mat_orange = make_material("mat_re_orange", NARANJA_ACENTO)

    body = make_cylinder("body", 0.8, 1.0, segments=8, location=(0, 0, 0.5), mat=mat_copper)
    bell = make_cone("bell", 0.8, 1.2, 0.8, segments=8, location=(0, 0, -0.4), rotation=(math.pi, 0, 0), mat=mat_metal)
    for i, ang in enumerate([20, -20]):
        side = make_cone(f"side_{i}", 0.3, 0.4, 0.5, segments=6, location=(math.sin(math.radians(ang)) * 0.5, 0, -0.1), rotation=(math.radians(ang), 0, 0), mat=mat_metal)
        parent_keep_transform(side, body)
    fin = make_cube("fin", scale=(0.5, 0.06, 0.6), location=(0.85, 0, 0.3), mat=mat_dark)
    return join_objects([body, bell, fin], "rocket_module_engine")


def make_rocket_cargo():
    mat_metal = make_material("mat_rc_metal", METAL_GRIS)
    mat_dark = make_material("mat_rc_dark", METAL_OSCURO)
    body = make_cylinder("body", 0.8, 2.0, segments=8, location=(0, 0, 1.0), mat=mat_metal)
    door = make_cube("door", scale=(0.7, 1.2, 0.1), location=(0, 0.85, 1.0), mat=mat_dark)
    parent_keep_transform(door, body)
    brace = make_cube("brace", scale=(0.08, 1.8, 0.08), location=(0, 0, 1.0), mat=mat_dark)
    return join_objects([body, brace], "rocket_module_cargo")


def make_landing_strut():
    mat_metal = make_material("mat_strut_metal", METAL_GRIS)
    mat_dark = make_material("mat_strut_dark", METAL_OSCURO)
    arm = make_cylinder("arm", 0.06, 2.0, segments=6, location=(0, 0, 1.0), rotation=(math.radians(45), 0, 0), mat=mat_metal)
    shock = make_cylinder("shock", 0.12, 0.4, segments=8, location=(0, 0, 0.8), mat=mat_dark)
    foot = make_cylinder("foot", 0.4, 0.06, segments=12, location=(0.9, 0, 0.03), mat=mat_metal)
    return join_objects([arm, shock, foot], "landing_strut")


def make_moon_boulder():
    mat_luna = make_material("mat_moon_rock", LUNA_GRIS)
    mat_dust = make_material("mat_moon_dust", LUNA_OSCURO)
    body = make_icosphere("body", 1.4, subdiv=1, location=(0, 0, 0.9), scale=(1, 0.85, 0.7), mat=mat_luna)
    displace_verts(body, 0.5, 99)
    dust = make_cylinder("dust", 1.2, 0.05, segments=8, location=(0, 0, 0.03), mat=mat_dust)
    return join_objects([body, dust], "moon_boulder")


def make_moon_crater():
    mat_luna = make_material("mat_crater", LUNA_GRIS)
    mat_dark = make_material("mat_crater_dark", LUNA_OSCURO)
    rim = make_torus("rim", 2.5, 0.45, major_segments=16, minor_segments=6, location=(0, 0, 0.2), mat=mat_luna)
    rim.scale.z = 0.3
    displace_verts(rim, 0.3, 77)
    floor = make_cylinder("floor", 2.0, 0.05, segments=12, location=(0, 0, 0.05), mat=mat_dark)
    return join_objects([rim, floor], "moon_crater_rim")


def make_solar_array():
    mat_metal = make_material("mat_array_metal", METAL_GRIS)
    mat_panel = make_material("mat_array_panel", AZUL_PANEL, roughness=0.05, metallic=0.1)
    mat_dark = make_material("mat_array_dark", METAL_OSCURO)

    spine = make_cube("spine", scale=(0.08, 0.08, 4.0), location=(0, 0, 2.0), mat=mat_metal)
    panel_root = create_empty("panel")
    parent_keep_transform(panel_root, spine)
    panel_root.location = (0, 0, 2.0)

    for side, x_sign in [("l", -1), ("r", 1)]:
        arm = make_cube(f"arm_{side}", scale=(0.08, 0.08, 3.0), location=(x_sign * 1.5, 0, 0), mat=mat_metal)
        parent_keep_transform(arm, panel_root)
        for i in range(3):
            p = make_cube(
                f"panel_{side}_{i}",
                scale=(0.8, 0.06, 1.2),
                location=(x_sign * 1.5, 0, -1.0 + i * 1.0),
                mat=mat_panel,
            )
            parent_keep_transform(p, panel_root)

    joint = make_uv_sphere("joint", 0.15, segments=6, ring_count=3, location=(0, 0, 2.0), mat=mat_dark)
    parent_keep_transform(joint, spine)
    return spine


def make_comms_dish():
    mat_metal = make_material("mat_comms_metal", METAL_GRIS, roughness=0.3, metallic=0.5)
    mat_dark = make_material("mat_comms_dark", METAL_OSCURO)

    base = make_cube("base", scale=(0.6, 0.3, 0.6), location=(0, 0, 0.15), mat=mat_metal)
    mast = make_cylinder("mast", 0.07, 1.0, segments=6, location=(0, 0, 0.8), mat=mat_metal)
    dish = create_empty("dish")
    parent_keep_transform(dish, mast)
    dish.location = (0, 0, 1.3)
    plate = make_icosphere("plate", 1.2, subdiv=1, location=(0, 0, 0), scale=(1, 1, 0.2), mat=mat_metal)
    rx = make_cylinder("rx", 0.04, 0.4, segments=4, location=(0, 0, 0.15), mat=mat_dark)
    parent_keep_transform(plate, dish)
    parent_keep_transform(rx, dish)
    parent_keep_transform(mast, base)
    return base


def make_lunar_rover():
    mat_metal = make_material("mat_rover_metal", METAL_GRIS, roughness=0.4, metallic=0.3)
    mat_panel = make_material("mat_rover_panel", AZUL_PANEL)
    mat_dark = make_material("mat_rover_dark", METAL_OSCURO)

    chassis = make_cube("chassis", scale=(1.8, 0.5, 1.0), location=(0, 0, 0.55), mat=mat_metal)
    solar = make_cube("solar", scale=(1.6, 0.06, 0.8), location=(0, 0, 1.05), mat=mat_panel)
    parent_keep_transform(solar, chassis)

    wheel_specs = [
        ("wheel_fl", (-0.7, 0.45, 0.3)),
        ("wheel_fr", (0.7, 0.45, 0.3)),
        ("wheel_rl", (-0.7, -0.45, 0.3)),
        ("wheel_rr", (0.7, -0.45, 0.3)),
    ]
    for name, (x, y, z) in wheel_specs:
        w = make_cylinder(name, 0.3, 0.2, segments=8, location=(x, y, z), rotation=(0, math.pi / 2, 0), mat=mat_dark)
        parent_keep_transform(w, chassis)

    sensor = make_cylinder("sensor", 0.03, 0.5, segments=4, location=(0, 0.55, 0.7), rotation=(math.radians(25), 0, 0), mat=mat_metal)
    parent_keep_transform(sensor, chassis)
    return chassis


# (catalog_kind, builder, staging_only)
# staging_only=True → tools/blender/staging/ (kind not in catalog-kinds.txt yet)
ASSET_REGISTRY = [
    ("flora_tree_oak", make_tree_oak, False),
    ("flora_tree_pine", make_tree_birch, False),
    ("rock_medium", make_rock_medium, False),
    ("rock_boulder", make_rock_large, False),
    ("flora_bush", make_shrub_bush, False),
    ("flora_grass_tuft", make_grass_tuft, False),
    ("item_mat_log", make_wood_log, False),
    ("prop_lantern", make_campfire, False),
    ("flora_flower_red", lambda: make_flower(ROJO_ACENTO, "flower_red"), False),
    ("flora_flower_yellow", lambda: make_flower(AMARILLO_ACENTO, "flower_yellow"), False),
    ("prop_pillar", make_stone_pillar, False),
    ("prop_crate", make_chest_wood, False),
    ("building_cottage", make_cabin_small, False),
    ("machine_wood_extractor", make_machine_wood_extractor, False),
    ("machine_stone_extractor", make_machine_stone_drill, False),
    ("machine_pump", make_machine_pump, True),
    ("habitation_dome", make_habitation_dome, True),
    ("solar_panel", make_solar_panel, True),
    ("prop_crate_large", make_storage_box_small, False),
    ("machine_launch_pad", make_launch_pad, False),
    ("prop_sign", make_antenna_simple, False),
    ("prop_barrel", make_fuel_tank_tall, False),
    ("rocket_module_cabin", make_rocket_cabin, True),
    ("rocket_module_fuel", make_rocket_fuel, True),
    ("rocket_module_engine", make_rocket_engine, True),
    ("rocket_module_cargo", make_rocket_cargo, True),
    ("landing_strut", make_landing_strut, True),
    ("moon_boulder", make_moon_boulder, True),
    ("moon_crater_rim", make_moon_crater, True),
    ("solar_array_panel", make_solar_array, True),
    ("comms_dish", make_comms_dish, True),
    ("lunar_rover", make_lunar_rover, True),
]


def main():
    print("=" * 60)
    print("AGENCIA ESPACIAL SURVIVAL — Asset Generator v1.0")
    print("NOTA: Script de herramienta externa. Ver comentario al inicio.")
    print(f"Blender {bpy.app.version_string}")
    print(f"Game output: {OUTPUT_DIR}")
    print(f"Staging:     {STAGING_DIR}")
    print("=" * 60)

    results = []
    errors = []
    warnings = []

    for i, (kind, fn, staging_only) in enumerate(ASSET_REGISTRY):
        clear_scene()
        out_dir = STAGING_DIR if staging_only else OUTPUT_DIR
        print(f"[{i + 1}/{len(ASSET_REGISTRY)}] {kind}{' (staging)' if staging_only else ''}...", end=" ")
        try:
            root = fn()
            set_origin_to_base(root)
            stats = export_asset(root, kind, out_dir)
            results.append(stats)
            tri = stats["triangles"]
            flag = " ✓" if tri <= MAX_TRIS_WARN else f" ⚠ {tri} tris (>{MAX_TRIS_WARN})"
            if tri > MAX_TRIS_WARN:
                warnings.append((kind, tri))
            print(f"{tri} tris{flag}")
        except Exception as e:
            errors.append((kind, str(e)))
            print(f"ERROR: {e}")
            traceback.print_exc()

    print("\n" + "=" * 60)
    print(f"COMPLETADO: {len(results)}/{len(ASSET_REGISTRY)} assets generados")
    if results:
        avg = sum(r["triangles"] for r in results) // len(results)
        print(f"Triángulos promedio: {avg}")
    game_count = sum(1 for _, _, st in ASSET_REGISTRY if not st)
    print(f"En juego ({OUTPUT_DIR}): {game_count} kinds catalogados")
    print(f"Staging ({STAGING_DIR}): {len(ASSET_REGISTRY) - game_count} kinds futuros")
    if warnings:
        print(f"\nADVERTENCIAS triángulos > {MAX_TRIS_WARN}:")
        for kind, tri in warnings:
            print(f"  - {kind}: {tri}")
    if errors:
        print(f"\nERRORES ({len(errors)}):")
        for kind, err in errors:
            print(f"  - {kind}: {err}")
        sys.exit(1)
    print("=" * 60)


if __name__ == "__main__":
    main()
