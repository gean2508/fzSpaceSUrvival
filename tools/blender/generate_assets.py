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

import json
import math
import os
import random
import sys
import traceback
from argparse import Namespace

import bpy
import bmesh
from mathutils import Matrix, Vector

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
PROJECT_ROOT = os.path.normpath(os.path.join(SCRIPT_DIR, "..", ".."))
OUTPUT_DIR = os.path.join(PROJECT_ROOT, "native", "crates", "ha-game", "assets", "models")
STAGING_DIR = os.path.join(SCRIPT_DIR, "staging")
MANUAL_DIR = os.path.join(SCRIPT_DIR, "manual")
EDITABLE_DIR = os.path.join(SCRIPT_DIR, "editable")
OVERRIDES_DIR = os.path.join(MANUAL_DIR, "overrides")
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
TIERRA = "#8b6914"
AUTUMN_LEAF = "#c4782a"
CHERRY_PINK = "#f4a6c8"
MAPLE_RED = "#b83232"
CERULEAN = "#4a90d9"
FOG_WHITE = "#e8e4dc"
BARN_RED = "#a63d2f"
HERO_BLUE = "#3b6ea5"
FARMER_GREEN = "#4a7c3f"
ELDER_GRAY = "#7a8a8f"
GUARD_RED = "#8b2e2e"
LEATHER_TAN = "#8b5a2b"
POTION_GREEN = "#2ecc71"
POTION_PURPLE = "#9b59b6"
POTION_BLUE = "#3498db"
NEON_CYAN = "#00e5ff"
NEON_PINK = "#ff6eb4"
SLIME_GREEN = "#5cdb6a"
GOLEM_STONE = "#6b6b78"
SPIRIT_LAVENDER = "#b8a9e8"

CATALOG_KINDS_PATH = os.path.join(
    PROJECT_ROOT, "native", "crates", "ha-game", "assets", "models", "catalog-kinds.txt"
)

# Kinds not in catalog-kinds.txt — export to tools/blender/staging/
STAGING_ONLY_KINDS = frozenset(
    {
        "machine_pump",
        "habitation_dome",
        "solar_panel",
        "rocket_module_cabin",
        "rocket_module_fuel",
        "rocket_module_engine",
        "rocket_module_cargo",
        "landing_strut",
        "moon_boulder",
        "moon_crater_rim",
        "solar_array_panel",
        "comms_dish",
        "lunar_rover",
        "prop_campfire",
        "flora_tree_birch",
    }
)


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
    """Parent while preserving the child's current world transform."""
    world = child.matrix_world.copy()
    child.parent = parent
    child.matrix_world = world


def parent_local(child, parent, location=(0, 0, 0), rotation=(0, 0, 0), scale=None):
    """Parent then place child in parent's local space."""
    parent_keep_transform(child, parent)
    child.location = Vector(location)
    child.rotation_euler = rotation
    if scale is not None:
        child.scale = Vector(scale)


def create_empty(name: str):
    obj = bpy.data.objects.new(name, None)
    link_object(obj)
    return obj


def make_root(name: str):
    return create_empty(name)


def parent_to_root(root, *children):
    for child in children:
        parent_keep_transform(child, root)


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


def clip_mesh_below_z(obj, z_cut):
    """Keep only geometry at/above z_cut (removes verts below the cut plane)."""
    if obj.type != "MESH":
        return
    bpy.context.view_layer.update()
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
        export_vertex_color="NONE",
    )

    return {"kind": kind, "path": output_path, "triangles": total_tris}


# --- Manual overrides (hand-tuned in Blender GUI) ----------------------------


def parse_cli_args() -> Namespace:
    """Args after '--' when invoked: blender --background --python generate_assets.py -- [args]"""
    argv = sys.argv
    args = argv[argv.index("--") + 1 :] if "--" in argv else []
    only: set[str] | None = None
    save_blend: list[str] = []
    capture: str | None = None
    i = 0
    while i < len(args):
        token = args[i]
        if token == "--force":
            pass
        elif token == "--only" and i + 1 < len(args):
            only = {k.strip() for k in args[i + 1].split(",") if k.strip()}
            i += 1
        elif token == "--save-blend" and i + 1 < len(args):
            save_blend.append(args[i + 1].strip())
            i += 1
        elif token == "--capture" and i + 1 < len(args):
            capture = args[i + 1].strip()
            i += 1
        i += 1
    return Namespace(
        force="--force" in args,
        only=only,
        save_blend=save_blend,
        capture=capture,
    )


def manual_blend_path(kind: str) -> str | None:
    path = os.path.join(MANUAL_DIR, f"{kind}.blend")
    return path if os.path.isfile(path) else None


def editable_blend_path(kind: str) -> str:
    return os.path.join(EDITABLE_DIR, f"{kind}.blend")


def override_json_path(kind: str) -> str:
    return os.path.join(OVERRIDES_DIR, f"{kind}.json")


def load_transform_overrides(kind: str) -> dict | None:
    path = override_json_path(kind)
    if not os.path.isfile(path):
        return None
    with open(path, encoding="utf-8") as f:
        data = json.load(f)
    if data.get("kind") and data["kind"] != kind:
        print(f"  AVISO: override JSON kind={data['kind']} != {kind}")
    return data


def apply_transform_overrides(root, overrides: dict) -> bool:
    """Apply saved local transforms by object name. Returns skip_origin_reset flag."""
    by_name = {o.name: o for o in iter_hierarchy(root)}
    for name, tf in overrides.get("objects", {}).items():
        obj = by_name.get(name)
        if obj is None:
            continue
        if "location" in tf:
            obj.location = Vector(tf["location"])
        if "rotation" in tf:
            obj.rotation_euler = tf["rotation"]
        if "scale" in tf:
            obj.scale = Vector(tf["scale"])
    return bool(overrides.get("skip_origin_reset", True))


def capture_transform_overrides(kind: str, root) -> str:
    """Serialize hierarchy local transforms → manual/overrides/{kind}.json"""
    os.makedirs(OVERRIDES_DIR, exist_ok=True)
    data = {
        "version": 1,
        "kind": kind,
        "skip_origin_reset": True,
        "objects": {},
    }
    for obj in iter_hierarchy(root):
        data["objects"][obj.name] = {
            "location": [obj.location.x, obj.location.y, obj.location.z],
            "rotation": [obj.rotation_euler.x, obj.rotation_euler.y, obj.rotation_euler.z],
            "scale": [obj.scale.x, obj.scale.y, obj.scale.z],
        }
    path = override_json_path(kind)
    with open(path, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=2)
        f.write("\n")
    return path


def find_export_root(kind: str):
    """Root object for export from a hand-edited .blend file."""
    for name in (kind, f"{kind}_root", "export_root"):
        if name in bpy.data.objects:
            return bpy.data.objects[name]
    roots = [o for o in bpy.context.scene.objects if o.parent is None]
    if len(roots) == 1:
        return roots[0]
    if not roots:
        raise RuntimeError(f"Manual blend vacío para '{kind}'")
    names = ", ".join(o.name for o in roots[:6])
    raise RuntimeError(
        f"Manual blend '{kind}': agrupa todo bajo un Empty llamado '{kind}' "
        f"(hay {len(roots)} raíces: {names})"
    )


def save_blend_for_editing(root, kind: str) -> str:
    """Guarda el asset procedural como .blend editable en tools/blender/editable/."""
    os.makedirs(EDITABLE_DIR, exist_ok=True)
    path = editable_blend_path(kind)

    def select_hierarchy(obj):
        obj.select_set(True)
        for child in obj.children:
            select_hierarchy(child)

    bpy.ops.object.select_all(action="DESELECT")
    select_hierarchy(root)
    bpy.context.view_layer.objects.active = root
    bpy.ops.wm.save_as_mainfile(filepath=path)
    return path


def export_from_manual_blend(kind: str, out_dir: str, blend_path: str) -> dict:
    """Export GLB from a hand-edited .blend (no regeneración procedural)."""
    bpy.ops.wm.open_mainfile(filepath=blend_path)
    bpy.context.view_layer.update()
    root = find_export_root(kind)
    overrides = load_transform_overrides(kind)
    if overrides:
        apply_transform_overrides(root, overrides)
    elif not os.path.isfile(override_json_path(kind)):
        pass  # manual blend is source of truth
    return export_asset(root, kind, out_dir)


def generate_asset_kind(kind: str, fn, staging_only: bool, opts: Namespace) -> dict:
    out_dir = STAGING_DIR if staging_only else OUTPUT_DIR
    blend_path = manual_blend_path(kind)

    if blend_path and not opts.force:
        return export_from_manual_blend(kind, out_dir, blend_path)

    clear_scene()
    root = fn()
    source = "procedural"

    override_path = override_json_path(kind)
    skip_origin = False
    if os.path.isfile(override_path) and not opts.force:
        overrides = load_transform_overrides(kind)
        if overrides:
            skip_origin = apply_transform_overrides(root, overrides)
            source = "procedural+override"

    if not skip_origin:
        set_origin_to_base(root)

    if kind in opts.save_blend:
        blend_out = save_blend_for_editing(root, kind)
        print(f"  → editable: {blend_out}")

    stats = export_asset(root, kind, out_dir)
    stats["source"] = source
    return stats


def run_capture_mode(kind: str):
    """Captura transforms del .blend abierto o editable/{kind}.blend → overrides JSON."""
    blend_manual = manual_blend_path(kind)
    blend_editable = editable_blend_path(kind)

    if bpy.context.scene.objects:
        root = find_export_root(kind)
    elif blend_manual:
        bpy.ops.wm.open_mainfile(filepath=blend_manual)
        root = find_export_root(kind)
    elif os.path.isfile(blend_editable):
        bpy.ops.wm.open_mainfile(filepath=blend_editable)
        root = find_export_root(kind)
    else:
        clear_scene()
        if kind not in KIND_BUILDERS:
            raise KeyError(f"Sin builder ni .blend para '{kind}'")
        root = KIND_BUILDERS[kind]()
        print(f"  (escena generada proceduralmente para capturar — guarda editable si quieres)")

    path = capture_transform_overrides(kind, root)
    print(f"Overrides guardados: {path}")
    print("El próximo generate:assets aplicará estos transforms (usa --force para ignorar).")


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
    parent_keep_transform(snow_ring, trunk)

    canopy = create_empty("canopy")
    parent_keep_transform(canopy, trunk)
    canopy.location = (0, 0, 3.0)

    c1 = make_icosphere("c1", 2.0, subdiv=1, location=(0, 0, 0), mat=mat_leaf)
    snow1 = make_icosphere("snow1", 2.05, subdiv=1, location=(0, 0, 0), mat=mat_snow)
    c2 = make_icosphere("c2", 1.4, subdiv=1, location=(0, 0, 0), mat=mat_leaf)
    snow2 = make_icosphere("snow2", 1.45, subdiv=1, location=(0, 0, 0), mat=mat_snow)
    c3 = make_cone("c3", 0.6, 0.0, 1.0, segments=8, location=(0, 0, 0), mat=mat_leaf)

    parent_local(c1, canopy, (0, 0, 0.5), scale=(1, 1, 0.8))
    parent_local(snow1, canopy, (0, 0, 1.5))
    parent_local(c2, canopy, (0, 0, 2.0))
    parent_local(snow2, canopy, (0, 0, 2.5))
    parent_local(c3, canopy, (0, 0, 3.2))

    clip_mesh_below_z(snow1, 5.85)
    clip_mesh_below_z(snow2, 6.75)

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

    crown = make_icosphere("crown", 1.5, subdiv=1, location=(0, 0, 0), mat=mat_leaf)
    snow = make_icosphere("snow_cap", 1.55, subdiv=1, location=(0, 0, 0), mat=mat_snow)
    parent_local(crown, canopy, (0, 0, 0.5), scale=(0.8, 0.8, 1.1))
    parent_local(snow, canopy, (0, 0, 1.3))
    clip_mesh_below_z(snow, trunk.location.z + 5.8)

    for i, ang in enumerate([30, 45, -20]):
        rad = math.radians(ang)
        branch = make_cylinder(f"branch_{i}", 0.05, 1.2, segments=6, location=(0, 0, 0), mat=mat_trunk)
        parent_local(
            branch,
            canopy,
            (math.sin(rad) * 0.4, math.cos(rad) * 0.4, -0.35),
            rotation=(0, rad, math.pi / 2),
        )

    return trunk


def make_tree_pine():
    mat_trunk = make_material("mat_trunk_pine", MADERA_OSCURA)
    mat_snow = make_material("mat_snow_pine", NIEVE)
    mat_leaf = make_material("mat_leaf_pine", VERDE_BOSQUE)

    trunk = make_cylinder("trunk", 0.22, 4.0, segments=6, location=(0, 0, 2.0), mat=mat_trunk)
    displace_verts(trunk, 0.03, 17)

    canopy = create_empty("canopy")
    parent_keep_transform(canopy, trunk)
    canopy.location = (0, 0, 3.8)

    tiers = [
        (1.6, 0.0, 0.85),
        (1.25, 1.1, 0.75),
        (0.9, 2.1, 0.65),
        (0.45, 2.85, 0.5),
    ]
    for i, (radius, z, depth) in enumerate(tiers):
        cone = make_cone(f"tier_{i}", radius, 0.0, depth, segments=8, location=(0, 0, 0), mat=mat_leaf)
        parent_local(cone, canopy, (0, 0, z))
        snow = make_cylinder(f"snow_{i}", radius * 0.82, 0.05, segments=8, location=(0, 0, 0), mat=mat_snow)
        parent_local(snow, canopy, (0, 0, z + depth - 0.02))

    return trunk


def make_tree_oak_autumn():
    mat_trunk = make_material("mat_trunk_oak_aut", MADERA_OSCURA)
    mat_snow = make_material("mat_snow_oak_aut", NIEVE)
    mat_leaf = make_material("mat_leaf_oak_aut", AUTUMN_LEAF)

    trunk = make_cylinder("trunk", 0.35, 3.0, segments=8, location=(0, 0, 1.5), mat=mat_trunk)
    displace_verts(trunk, 0.05, 13)
    snow_ring = make_cylinder("snow_ring", 0.38, 0.04, segments=8, location=(0, 0, 1.5), mat=mat_snow)
    parent_keep_transform(snow_ring, trunk)

    canopy = create_empty("canopy")
    parent_keep_transform(canopy, trunk)
    canopy.location = (0, 0, 3.0)

    c1 = make_icosphere("c1", 2.0, subdiv=1, location=(0, 0, 0), mat=mat_leaf)
    c2 = make_icosphere("c2", 1.4, subdiv=1, location=(0, 0, 0), mat=mat_leaf)
    c3 = make_cone("c3", 0.6, 0.0, 1.0, segments=8, location=(0, 0, 0), mat=mat_leaf)
    parent_local(c1, canopy, (0, 0, 0.5), scale=(1, 1, 0.8))
    parent_local(c2, canopy, (0, 0, 2.0))
    parent_local(c3, canopy, (0, 0, 3.2))
    return trunk


def make_tree_cherry():
    mat_trunk = make_material("mat_cherry_trunk", TRONCO_NIEVE)
    mat_mark = make_material("mat_cherry_mark", MADERA_OSCURA)
    mat_blossom = make_material("mat_cherry_blossom", CHERRY_PINK)
    mat_blossom_light = make_material("mat_cherry_blossom_lt", "#f8c8dc")

    trunk_h = 4.2
    trunk = make_cylinder("trunk", 0.16, trunk_h, segments=6, location=(0, 0, trunk_h / 2), mat=mat_trunk)
    for i, z in enumerate([1.0, 2.0, 3.0]):
        mark = make_cube(f"mark_{i}", scale=(0.18, 0.04, 0.35), location=(0.18, 0, z), mat=mat_mark)
        parent_keep_transform(mark, trunk)

    canopy = create_empty("canopy")
    parent_keep_transform(canopy, trunk)
    canopy.location = (0, 0, trunk_h)

    crown = make_icosphere("crown", 1.5, subdiv=1, location=(0, 0, 0), mat=mat_blossom)
    parent_local(crown, canopy, (0, 0, 0.55), scale=(0.95, 0.95, 0.8))

    blossom_offsets = [
        (0.55, 0.15, 0.75, mat_blossom_light),
        (-0.45, 0.25, 0.9, mat_blossom),
        (0.15, -0.5, 0.65, mat_blossom_light),
        (-0.2, 0.45, 1.05, mat_blossom),
        (0.35, -0.2, 1.1, mat_blossom_light),
    ]
    for i, (x, y, z, mat) in enumerate(blossom_offsets):
        petal = make_uv_sphere(f"blossom_{i}", 0.1, segments=4, ring_count=2, location=(0, 0, 0), mat=mat)
        parent_local(petal, canopy, (x, y, z))

    return trunk


def make_tree_red():
    mat_trunk = make_material("mat_trunk_maple", MADERA_OSCURA)
    mat_leaf = make_material("mat_leaf_maple", MAPLE_RED)

    trunk = make_cylinder("trunk", 0.3, 3.2, segments=8, location=(0, 0, 1.6), mat=mat_trunk)
    displace_verts(trunk, 0.04, 19)
    canopy = create_empty("canopy")
    parent_keep_transform(canopy, trunk)
    canopy.location = (0, 0, 3.1)

    c1 = make_icosphere("c1", 1.8, subdiv=1, location=(0, 0, 0), mat=mat_leaf)
    c2 = make_icosphere("c2", 1.2, subdiv=1, location=(0, 0, 0), mat=mat_leaf)
    c3 = make_cone("c3", 0.5, 0.0, 0.9, segments=8, location=(0, 0, 0), mat=mat_leaf)
    parent_local(c1, canopy, (0, 0, 0.4), scale=(1, 1, 0.75))
    parent_local(c2, canopy, (0, 0, 1.8))
    parent_local(c3, canopy, (0, 0, 2.8))
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
    root = make_root("flora_bush")
    for i, (pos, r) in enumerate([((0, 0, 0.45), 0.55), ((0.4, 0.2, 0.4), 0.65), ((-0.3, 0.3, 0.38), 0.5)]):
        p = make_icosphere(f"b{i}", r, subdiv=1, location=(0, 0, 0), mat=mat_leaf)
        parent_local(p, root, pos, scale=(1, 1, 0.8))
        s = make_cylinder(f"s{i}", r * 0.88, 0.04, segments=6, location=(0, 0, 0), mat=mat_snow)
        parent_local(s, root, (pos[0], pos[1], pos[2] + r * 0.68))
    return root


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
            0.04,
            segments=6,
            location=(math.cos(ang) * 0.6, math.sin(ang) * 0.6, 0.02),
            mat=mat_stone,
        )
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


def make_crate_wood():
    """Wooden storage crate (prop_crate) — distinct from chest with lid."""
    mat_light = make_material("mat_crate_light", MADERA_CLARA)
    mat_dark = make_material("mat_crate_dark", MADERA_OSCURA)
    mat_metal = make_material("mat_crate_metal", METAL_GRIS)

    root = create_empty("prop_crate")
    body = make_cube("body", scale=(0.7, 0.7, 0.7), location=(0, 0, 0.35), mat=mat_light)
    parent_keep_transform(body, root)
    for z in (0.12, 0.58):
        slat = make_cube(f"slat_z{z}", scale=(0.72, 0.72, 0.04), location=(0, 0, z), mat=mat_dark)
        parent_keep_transform(slat, root)
    for x in (-0.28, 0.28):
        brace = make_cube(f"brace_{x}", scale=(0.04, 0.72, 0.72), location=(x, 0, 0.35), mat=mat_dark)
        parent_keep_transform(brace, root)
    corner = make_cube("corner_metal", scale=(0.08, 0.08, 0.08), location=(0.32, 0.32, 0.66), mat=mat_metal)
    parent_keep_transform(corner, root)
    return root


def make_lantern():
    mat_metal = make_material("mat_lantern_metal", METAL_GRIS)
    mat_dark = make_material("mat_lantern_dark", METAL_OSCURO)
    mat_glass = make_material("mat_lantern_glass", AMARILLO_ACENTO, roughness=0.15)
    mat_flame = make_material("mat_lantern_flame", NARANJA_ACENTO, roughness=0.2)

    root = make_root("prop_lantern")
    post = make_cylinder("post", 0.04, 1.2, segments=6, location=(0, 0, 0.6), mat=mat_dark)
    hook = make_torus("hook", 0.06, 0.015, major_segments=8, minor_segments=4, location=(0, 0, 1.25), mat=mat_metal)
    cage = make_cylinder("cage", 0.15, 0.35, segments=6, location=(0, 0, 0.8), mat=mat_metal)
    glass = make_cylinder("glass", 0.12, 0.28, segments=6, location=(0, 0, 0.8), mat=mat_glass)
    cap = make_cone("cap", 0.18, 0.02, 0.12, segments=6, location=(0, 0, 1.02), mat=mat_dark)
    base_cap = make_cylinder("base_cap", 0.18, 0.05, segments=6, location=(0, 0, 0.6), mat=mat_dark)
    flame = make_cone("flame", 0.04, 0.0, 0.1, segments=5, location=(0, 0, 0.82), mat=mat_flame)
    parent_to_root(root, post, hook, cage, glass, cap, base_cap, flame)
    return root


def make_sign():
    mat_wood = make_material("mat_sign_wood", MADERA_CLARA)
    mat_dark = make_material("mat_sign_dark", MADERA_OSCURA)
    mat_metal = make_material("mat_sign_metal", METAL_GRIS)

    root = create_empty("prop_sign")
    post = make_cylinder("post", 0.06, 1.6, segments=6, location=(0, 0, 0.8), mat=mat_dark)
    parent_keep_transform(post, root)
    bracket = make_cube("bracket", scale=(0.35, 0.06, 0.06), location=(0, 0, 1.45), mat=mat_metal)
    parent_keep_transform(bracket, root)
    board = make_cube("board", scale=(0.9, 0.08, 0.55), location=(0, 0.08, 1.35), mat=mat_wood)
    parent_keep_transform(board, root)
    frame = make_cube("frame", scale=(0.95, 0.04, 0.6), location=(0, 0.1, 1.35), mat=mat_dark)
    parent_keep_transform(frame, root)
    chain_l = make_cylinder("chain_l", 0.01, 0.18, segments=4, location=(-0.25, 0, 1.52), mat=mat_metal)
    chain_r = make_cylinder("chain_r", 0.01, 0.18, segments=4, location=(0.25, 0, 1.52), mat=mat_metal)
    parent_keep_transform(chain_l, root)
    parent_keep_transform(chain_r, root)
    return root


def make_barrel():
    mat_wood = make_material("mat_barrel_wood", MADERA_CLARA)
    mat_dark = make_material("mat_barrel_dark", MADERA_OSCURA)
    mat_metal = make_material("mat_barrel_metal", METAL_GRIS)

    root = create_empty("prop_barrel")
    body = make_cylinder("body", 0.32, 0.7, segments=8, location=(0, 0, 0.35), mat=mat_wood)
    displace_verts(body, 0.02, 8)
    parent_keep_transform(body, root)
    for z in (0.08, 0.35, 0.62):
        band = make_cylinder(f"band_{z}", 0.34, 0.04, segments=8, location=(0, 0, z), mat=mat_metal)
        parent_keep_transform(band, root)
    top = make_cylinder("top", 0.32, 0.03, segments=8, location=(0, 0, 0.7), mat=mat_dark)
    bottom = make_cylinder("bottom", 0.32, 0.03, segments=8, location=(0, 0, 0.015), mat=mat_dark)
    parent_keep_transform(top, root)
    parent_keep_transform(bottom, root)
    return root


def make_chest_wood():
    mat_light = make_material("mat_chest_light", MADERA_CLARA)
    mat_dark = make_material("mat_chest_dark", MADERA_OSCURA)
    mat_metal = make_material("mat_chest_metal", METAL_GRIS)

    body = make_cube("body", scale=(1.0, 0.7, 0.7), location=(0, 0, 0.35), mat=mat_light)
    door = make_cube("door", scale=(1.0, 0.25, 0.7), location=(0, 0, 0), mat=mat_dark)
    parent_local(door, body, (0, -0.225, 0.48))

    for x in (-0.35, 0.35):
        hinge = make_cylinder("hinge", 0.04, 0.15, segments=4, location=(x, 0, 0.72), rotation=(math.pi / 2, 0, 0), mat=mat_metal)
        parent_keep_transform(hinge, body)
    latch = make_cube("latch", scale=(0.2, 0.05, 0.1), location=(0, 0.38, 0.83), mat=mat_metal)
    parent_keep_transform(latch, body)
    return body


def make_cabin_small():
    mat_wall = make_material("mat_cabin_wall", MADERA_CLARA)
    mat_log = make_material("mat_cabin_log", MADERA_OSCURA)
    mat_roof = make_material("mat_cabin_roof", MADERA_VETA)
    mat_snow = make_material("mat_cabin_snow", NIEVE)
    mat_stone = make_material("mat_cabin_stone", PIEDRA_OSCURA)
    mat_win = make_material("mat_cabin_win", AMARILLO_ACENTO, roughness=0.1)

    root = make_root("building_cottage")
    body = make_cube("body", scale=(4.2, 3.2, 3.2), location=(0, 0, 1.6), mat=mat_wall)
    parent_keep_transform(body, root)

    for i, z in enumerate([0.35, 0.85, 1.35, 1.85, 2.35]):
        log_h = make_cube(f"log_h_{i}", scale=(4.25, 3.22, 0.18), location=(0, 0, 0), mat=mat_log)
        parent_local(log_h, root, (0, 0, z + 0.1))

    roof_l = make_cube("roof_l", scale=(2.4, 0.15, 4.4), location=(0, 0, 0), mat=mat_roof)
    parent_local(roof_l, root, (-1.05, 0, 3.55), rotation=(0, math.radians(35), 0))
    roof_r = make_cube("roof_r", scale=(2.4, 0.15, 4.4), location=(0, 0, 0), mat=mat_roof)
    parent_local(roof_r, root, (1.05, 0, 3.55), rotation=(0, math.radians(-35), 0))
    snow_l = make_cube("snow_l", scale=(2.3, 0.18, 4.3), location=(0, 0, 0), mat=mat_snow)
    parent_local(snow_l, root, (-1.0, 0, 3.75), rotation=(0, math.radians(35), 0))
    snow_r = make_cube("snow_r", scale=(2.3, 0.18, 4.3), location=(0, 0, 0), mat=mat_snow)
    parent_local(snow_r, root, (1.0, 0, 3.75), rotation=(0, math.radians(-35), 0))

    chimney = make_cube("chimney", scale=(0.45, 1.1, 0.45), location=(0, 0, 0), mat=mat_stone)
    parent_local(chimney, root, (1.5, 0, 3.8))
    chimney_top = make_cylinder("chimney_top", 0.26, 0.12, segments=8, location=(0, 0, 0), mat=mat_stone)
    parent_local(chimney_top, root, (1.5, 0, 4.28))

    door = make_cube("door", scale=(1.0, 2.0, 0.1), location=(0, 0, 0), mat=mat_log)
    parent_local(door, body, (0, -1.55, -0.7))

    for x in (-1.4, 1.4):
        win_frame = make_cube(f"wframe_{x}", scale=(0.75, 0.1, 0.65), location=(0, 0, 0), mat=mat_log)
        parent_local(win_frame, root, (x, -1.55, 1.8))
        win_glass = make_cube(f"wglass_{x}", scale=(0.55, 0.08, 0.45), location=(0, 0, 0), mat=mat_win)
        parent_local(win_glass, root, (x, -1.52, 1.8))

    step = make_cube("step", scale=(1.2, 0.15, 0.35), location=(0, 0, 0), mat=mat_stone)
    parent_local(step, root, (0, -1.7, 0.18))
    return root


def make_machine_wood_extractor():
    mat_wood = make_material("mat_mwe_wood", MADERA_OSCURA)
    mat_metal = make_material("mat_mwe_metal", METAL_GRIS)
    mat_rust = make_material("mat_mwe_rust", METAL_OXIDADO)
    mat_rope = make_material("mat_mwe_rope", MADERA_VETA)

    root = make_root("machine_wood_extractor")
    base = make_cube("base", scale=(2.2, 0.35, 2.2), location=(0, 0, 0.175), mat=mat_wood)
    parent_keep_transform(base, root)

    leg_foot = [(-0.7, -0.7), (0.7, -0.7), (0.0, 0.8)]
    for i, (fx, fy) in enumerate(leg_foot):
        leg = make_cylinder(f"leg_{i}", 0.08, 2.8, segments=6, location=(0, 0, 0), mat=mat_wood)
        parent_local(leg, root, (fx * 0.5, fy * 0.5, 1.2))
        leg.rotation_euler = (math.atan2(-fy * 0.35, 2.8), math.atan2(-fx * 0.35, 2.8), 0)

    apex = make_uv_sphere("apex", 0.12, segments=6, ring_count=3, location=(0, 0, 2.55), mat=mat_metal)
    parent_keep_transform(apex, root)

    arm = create_empty("arm")
    parent_local(arm, root, (0, 0, 2.55))

    boom = make_cylinder("boom", 0.06, 1.8, segments=6, location=(0, 0, 0), mat=mat_wood)
    parent_local(boom, arm, (0.9, 0, 0), rotation=(0, math.pi / 2, 0))

    pulley = make_torus("pulley", 0.1, 0.025, major_segments=8, minor_segments=4, location=(0, 0, 0), mat=mat_metal)
    parent_local(pulley, arm, (1.8, 0, 0))

    rope = make_cylinder("rope", 0.015, 0.9, segments=4, location=(0, 0, 0), mat=mat_rope)
    parent_local(rope, arm, (1.8, 0, -0.45))

    hook = make_torus("hook", 0.06, 0.018, major_segments=6, minor_segments=3, location=(0, 0, 0), mat=mat_metal)
    parent_local(hook, arm, (1.8, 0, -0.95))

    rotor = make_cylinder("rotor", 0.38, 0.07, segments=10, location=(0, 0, 0), mat=mat_rust)
    parent_local(rotor, root, (0.8, 0, 0.45))
    for i in range(6):
        ang = i * math.pi / 3
        tooth = make_cube(f"tooth_{i}", scale=(0.06, 0.1, 0.07), location=(0, 0, 0), mat=mat_rust)
        parent_local(tooth, root, (0.8 + math.cos(ang) * 0.4, math.sin(ang) * 0.4, 0.45))

    return root


def make_machine_stone_drill():
    mat_metal = make_material("mat_msd_metal", METAL_GRIS)
    mat_rust = make_material("mat_msd_rust", METAL_OXIDADO)
    mat_dark = make_material("mat_msd_dark", METAL_OSCURO)
    mat_copper = make_material("mat_msd_copper", METAL_COBRE)
    mat_red = make_material("mat_msd_red", ROJO_ACENTO)

    root = make_root("machine_stone_extractor")
    base = make_cube("base", scale=(2.0, 0.45, 2.0), location=(0, 0, 0.225), mat=mat_metal)
    parent_keep_transform(base, root)

    boiler = make_cylinder("boiler", 0.65, 1.8, segments=6, location=(0, 0, 0), rotation=(math.pi / 2, 0, 0), mat=mat_rust)
    parent_local(boiler, root, (0, 0, 1.1))

    for y in (-0.92, 0.92):
        cap = make_cylinder(f"cap_{y}", 0.66, 0.08, segments=6, location=(0, 0, 0), rotation=(math.pi / 2, 0, 0), mat=mat_copper)
        parent_local(cap, root, (0, y, 1.1))

    stack = make_cylinder("stack", 0.14, 1.2, segments=6, location=(0, 0, 0), mat=mat_dark)
    parent_local(stack, root, (-0.35, 0, 1.95))
    stack_top = make_cone("stack_top", 0.18, 0.12, 0.12, segments=6, location=(0, 0, 0), mat=mat_dark)
    parent_local(stack_top, root, (-0.35, 0, 2.58))

    rotor = make_cylinder("rotor", 0.55, 0.1, segments=10, location=(0, 0, 0), mat=mat_dark)
    parent_local(rotor, root, (0.85, -0.6, 1.1))
    for i in range(6):
        ang = i * math.pi / 3
        tooth = make_cube(f"gtooth_{i}", scale=(0.07, 0.1, 0.1), location=(0, 0, 0), mat=mat_dark)
        parent_local(tooth, root, (0.85 + math.cos(ang) * 0.58, -0.6 + math.sin(ang) * 0.58, 1.1))

    gauge = make_cylinder("gauge", 0.13, 0.04, segments=8, location=(0, 0, 0), mat=mat_metal)
    parent_local(gauge, root, (0.68, -0.82, 1.35))
    needle = make_cylinder("needle", 0.01, 0.1, segments=4, location=(0, 0, 0), mat=mat_red)
    parent_local(needle, root, (0.68, -0.84, 1.38), rotation=(0, math.radians(35), 0))

    for side, x in [("l", -0.7), ("r", 0.7)]:
        pipe = make_cylinder(f"pipe_{side}", 0.06, 0.55, segments=6, location=(0, 0, 0), mat=mat_copper)
        parent_local(pipe, root, (x, 0, 0.68))

    drill_mount = make_cylinder("drill_mount", 0.22, 0.3, segments=6, location=(0, 0, 0), mat=mat_rust)
    parent_local(drill_mount, root, (0, 0, 0.4))
    drill = make_cone("drill", 0.18, 0.0, 0.75, segments=6, location=(0, 0, 0), rotation=(math.pi, 0, 0), mat=mat_metal)
    parent_local(drill, root, (0, 0, -0.1))

    return root


def make_machine_pump():
    mat_metal = make_material("mat_pump_metal", METAL_GRIS)
    mat_rust = make_material("mat_pump_rust", METAL_OXIDADO)

    root = make_root("machine_pump")
    base = make_cube("base", scale=(1.0, 0.3, 1.0), location=(0, 0, 0.15), mat=mat_metal)
    body = make_cylinder("body", 0.4, 1.2, segments=8, location=(0, 0, 0.9), mat=mat_rust)
    head = make_uv_sphere("head", 0.42, segments=8, ring_count=4, location=(0, 0, 1.55), mat=mat_rust)
    arm = make_cube("arm", scale=(0.12, 0.8, 0.12), location=(0, 0, 0.5), mat=mat_metal)
    parent_keep_transform(arm, body)
    pipe = make_cylinder("pipe", 0.08, 0.8, segments=4, location=(0.5, 0, 0.2), rotation=(0, 0, math.pi / 2), mat=mat_metal)
    parent_to_root(root, base, body, head, pipe)
    return root


def make_habitation_dome():
    mat_metal = make_material("mat_dome_metal", METAL_GRIS, roughness=0.4, metallic=0.3)
    mat_dark = make_material("mat_dome_dark", METAL_OSCURO)
    mat_glass = make_material("mat_dome_glass", AZUL_PANEL, roughness=0.1)

    root = make_root("habitation_dome")
    base = make_cylinder("base", 2.5, 0.6, segments=12, location=(0, 0, 0.3), mat=mat_metal)
    dome = make_icosphere("dome", 2.5, subdiv=1, location=(0, 0, 1.8), scale=(1, 1, 0.55), mat=mat_metal)
    clip_mesh_below_z(dome, 1.25)
    window = make_cylinder("window", 0.5, 0.05, segments=12, location=(0, 2.45, 1.5), rotation=(math.pi / 2, 0, 0), mat=mat_glass)
    door = make_cube("door", scale=(0.8, 1.5, 0.15), location=(0, 2.5, 0.75), mat=mat_dark)
    parent_keep_transform(door, base)
    tube = make_cylinder("tube", 0.5, 1.0, segments=8, location=(0, 3.2, 0.5), rotation=(0, 0, math.pi / 2), mat=mat_metal)
    parent_to_root(root, base, dome, window, tube)
    return root


def make_solar_panel():
    mat_metal = make_material("mat_solar_metal", METAL_GRIS)
    mat_dark = make_material("mat_solar_dark", METAL_OSCURO)
    mat_panel = make_material("mat_solar_panel", AZUL_PANEL, roughness=0.05, metallic=0.1)

    root = make_root("solar_panel")
    pole = make_cylinder("pole", 0.06, 1.2, segments=6, location=(0, 0, 0.6), mat=mat_metal)
    joint = make_uv_sphere("joint", 0.09, segments=6, ring_count=3, location=(0, 0, 1.2), mat=mat_dark)
    panel = make_cube("panel", scale=(1.2, 0.06, 2.0), location=(0, 0, 1.35), rotation=(math.radians(15), 0, 0), mat=mat_panel)
    parent_keep_transform(panel, pole)
    parent_to_root(root, pole, joint)
    return root


def make_storage_box_small():
    mat_metal = make_material("mat_box_metal", METAL_GRIS)
    mat_dark = make_material("mat_box_dark", METAL_OSCURO)
    mat_rust = make_material("mat_box_rust", METAL_OXIDADO)
    mat_accent = make_material("mat_box_accent", NARANJA_ACENTO)

    root = make_root("prop_crate_large")
    body = make_cube("body", scale=(1.0, 0.9, 1.0), location=(0, 0, 0.45), mat=mat_metal)
    door = make_cube("door", scale=(1.0, 0.15, 1.0), location=(0, 0, 0.98), mat=mat_dark)
    parent_keep_transform(door, body)
    cross_a = make_cube("cross_a", scale=(0.9, 0.04, 0.04), location=(0, 0, 0.55), mat=mat_rust)
    cross_b = make_cube("cross_b", scale=(0.04, 0.04, 0.9), location=(0, 0, 0.55), mat=mat_rust)
    lock = make_cube("lock", scale=(0.12, 0.05, 0.08), location=(0, 0.48, 0.55), mat=mat_accent)
    parent_to_root(root, body, cross_a, cross_b, lock)
    return root


def make_launch_pad():
    mat_metal = make_material("mat_pad_metal", METAL_GRIS, roughness=0.6, metallic=0.4)
    mat_dark = make_material("mat_pad_dark", METAL_OSCURO)
    mat_orange = make_material("mat_pad_orange", NARANJA_ACENTO)
    mat_yellow = make_material("mat_pad_yellow", AMARILLO_ACENTO)

    root = make_root("machine_launch_pad")
    platform = make_cube("platform", scale=(4.0, 0.4, 4.0), location=(0, 0, 0.2), mat=mat_metal)
    parent_to_root(root, platform)
    for x, z in [(-1.8, -1.8), (1.8, -1.8), (-1.8, 1.8), (1.8, 1.8)]:
        leg = make_cylinder(f"leg_{x}_{z}", 0.25, 0.8, segments=8, location=(x, 0, -0.2), mat=mat_dark)
        parent_keep_transform(leg, root)
    for x, z in [(-0.5, -0.5), (0.5, -0.5), (-0.5, 0.5), (0.5, 0.5)]:
        guide = make_cylinder(f"guide_{x}_{z}", 0.1, 1.5, segments=6, location=(x, 0, 0.95), mat=mat_orange)
        parent_keep_transform(guide, root)
    stripe = make_cube("stripe", scale=(3.8, 0.05, 0.2), location=(0, 0, 1.95), mat=mat_yellow)
    parent_keep_transform(stripe, root)
    return root


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
    top = make_uv_sphere("top", 0.55, segments=8, ring_count=3, location=(0, 0, 3.05), mat=mat_yellow)
    bottom = make_uv_sphere("bottom", 0.55, segments=8, ring_count=3, location=(0, 0, -0.05), mat=mat_yellow)
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

    root = make_root("rocket_module_cabin")
    body = make_cylinder("body", 0.8, 2.0, segments=8, location=(0, 0, 1.0), mat=mat_copper)
    nose = make_cone("nose", 0.8, 0.0, 0.8, segments=8, location=(0, 0, 2.4), mat=mat_copper)
    for i, ang in enumerate([0, 2.1, 4.2]):
        w = make_cylinder(
            f"win_{i}",
            0.2,
            0.05,
            segments=8,
            location=(math.cos(ang) * 0.8, math.sin(ang) * 0.8, 1.2),
            rotation=(0, ang, math.pi / 2),
            mat=mat_glass,
        )
        parent_keep_transform(w, body)
    band = make_cylinder("band", 0.82, 0.15, segments=8, location=(0, 0, 0.2), mat=mat_rust)
    parent_to_root(root, body, nose, band)
    return root


def make_rocket_fuel():
    mat_copper = make_material("mat_rf_copper", METAL_COBRE)
    mat_rust = make_material("mat_rf_rust", METAL_OXIDADO)
    mat_metal = make_material("mat_rf_metal", METAL_GRIS)

    root = make_root("rocket_module_fuel")
    body = make_cylinder("body", 0.8, 2.5, segments=8, location=(0, 0, 1.25), mat=mat_copper)
    for z in (0.4, 2.1):
        band = make_cylinder(f"band_{z}", 0.82, 0.12, segments=8, location=(0, 0, z), mat=mat_rust)
        parent_keep_transform(band, body)
    valve = make_cube("valve", scale=(0.15, 0.3, 0.15), location=(0.85, 0, 1.5), mat=mat_metal)
    parent_to_root(root, body, valve)
    return root


def make_rocket_engine():
    mat_copper = make_material("mat_re_copper", METAL_COBRE)
    mat_metal = make_material("mat_re_metal", METAL_GRIS)
    mat_dark = make_material("mat_re_dark", METAL_OSCURO)

    root = make_root("rocket_module_engine")
    body = make_cylinder("body", 0.8, 1.0, segments=8, location=(0, 0, 0.5), mat=mat_copper)
    bell = make_cone("bell", 1.2, 0.8, 0.8, segments=8, location=(0, 0, -0.4), mat=mat_metal)
    for i, ang in enumerate([20, -20]):
        side = make_cone(
            f"side_{i}",
            0.3,
            0.4,
            0.5,
            segments=6,
            location=(math.sin(math.radians(ang)) * 0.5, 0, -0.1),
            rotation=(math.radians(ang), 0, 0),
            mat=mat_metal,
        )
        parent_keep_transform(side, body)
    fin = make_cube("fin", scale=(0.5, 0.06, 0.6), location=(0.85, 0, 0.3), mat=mat_dark)
    parent_to_root(root, body, bell, fin)
    return root


def make_rocket_cargo():
    mat_metal = make_material("mat_rc_metal", METAL_GRIS)
    mat_dark = make_material("mat_rc_dark", METAL_OSCURO)

    body = make_cylinder("body", 0.8, 2.0, segments=8, location=(0, 0, 1.0), mat=mat_metal)
    door = make_cube("door", scale=(0.7, 1.2, 0.1), location=(0, 0.85, 1.0), mat=mat_dark)
    parent_keep_transform(door, body)
    brace = make_cube("brace", scale=(0.08, 1.8, 0.08), location=(0, 0, 1.0), mat=mat_dark)
    parent_keep_transform(brace, body)
    return body


def make_landing_strut():
    mat_metal = make_material("mat_strut_metal", METAL_GRIS)
    mat_dark = make_material("mat_strut_dark", METAL_OSCURO)

    root = make_root("landing_strut")
    arm = make_cylinder("arm", 0.06, 2.0, segments=6, location=(0, 0, 1.0), rotation=(math.radians(45), 0, 0), mat=mat_metal)
    shock = make_cylinder("shock", 0.12, 0.4, segments=8, location=(0, 0, 0.8), mat=mat_dark)
    foot = make_cylinder("foot", 0.4, 0.06, segments=12, location=(0.9, 0, 0.03), mat=mat_metal)
    parent_to_root(root, arm, shock, foot)
    return root


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
    rim = make_torus("rim", 2.5, 0.45, major_segments=12, minor_segments=5, location=(0, 0, 0.2), mat=mat_luna)
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
    parent_local(panel_root, spine, (0, 0, 2.0))

    for side, x_sign in [("l", -1), ("r", 1)]:
        arm = make_cube(f"arm_{side}", scale=(0.08, 0.08, 3.0), location=(0, 0, 0), mat=mat_metal)
        parent_local(arm, panel_root, (x_sign * 1.5, 0, 0))
        for i in range(2):
            p = make_cube(
                f"panel_{side}_{i}",
                scale=(0.8, 0.06, 1.2),
                location=(0, 0, 0),
                mat=mat_panel,
            )
            parent_local(p, panel_root, (x_sign * 1.5, 0, -0.5 + i * 1.0))

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

    root = make_root("lunar_rover")
    chassis = make_cube("chassis", scale=(1.8, 0.5, 1.0), location=(0, 0, 0.55), mat=mat_metal)
    solar = make_cube("solar", scale=(1.6, 0.06, 0.8), location=(0, 0, 0), mat=mat_panel)
    parent_local(solar, chassis, (0, 0, 0.5))

    wheel_specs = [
        ("wheel_fl", (-0.75, 0.38, 0.3)),
        ("wheel_fr", (0.75, 0.38, 0.3)),
        ("wheel_rl", (-0.75, -0.38, 0.3)),
        ("wheel_rr", (0.75, -0.38, 0.3)),
    ]
    for name, (x, y, z) in wheel_specs:
        w = make_cylinder(name, 0.3, 0.2, segments=8, location=(0, 0, 0), mat=mat_dark)
        parent_local(w, chassis, (x, y, z), rotation=(0, math.pi / 2, 0))

    sensor = make_cylinder("sensor", 0.03, 0.5, segments=4, location=(0, 0, 0), mat=mat_metal)
    parent_local(sensor, chassis, (0, 0.55, 0.7), rotation=(math.radians(25), 0, 0))
    parent_keep_transform(chassis, root)
    return root


# --- Catalog coverage (all kinds in catalog-kinds.txt) -----------------------


def make_terrain_tile(color_hex: str, name: str, height=0.25, width=1.0):
    root = make_root(name)
    tile = make_cube("tile", scale=(width, height, width), location=(0, 0, height / 2), mat=make_material(f"mat_{name}", color_hex))
    parent_to_root(root, tile)
    return root


def make_terrain_cliff():
    root = make_root("terrain_cliff")
    face = make_cube("face", scale=(1.4, 1.6, 0.6), location=(0, 0, 0.8), mat=make_material("mat_cliff", TIERRA))
    parent_to_root(root, face)
    return root


def make_terrain_slope():
    root = make_root("terrain_slope")
    slope = make_cube("slope", scale=(1.0, 0.25, 1.0), location=(0, 0, 0.125), rotation=(math.radians(-18), 0, 0), mat=make_material("mat_slope", VERDE_CLARO))
    parent_to_root(root, slope)
    return root


def make_shrub_small():
    mat_leaf = make_material("mat_shrub_small", VERDE_CLARO)
    root = make_root("flora_shrub")
    body = make_icosphere("body", 0.45, subdiv=1, location=(0, 0, 0.35), scale=(1, 1, 0.7), mat=mat_leaf)
    parent_to_root(root, body)
    return root


def make_mushroom(cap_color: str, name: str):
    mat_stem = make_material(f"mat_mush_stem_{name}", FOG_WHITE)
    mat_cap = make_material(f"mat_mush_cap_{name}", cap_color)
    root = make_root(name)
    stem = make_cylinder("stem", 0.04, 0.12, segments=6, location=(0, 0, 0.06), mat=mat_stem)
    cap = make_icosphere("cap", 0.14, subdiv=1, location=(0, 0, 0.16), scale=(1, 1, 0.55), mat=mat_cap)
    parent_to_root(root, stem, cap)
    return root


def make_rock_small():
    mat_stone = make_material("mat_rock_small", PIEDRA)
    root = make_root("rock_small")
    body = make_icosphere("rock", 0.45, subdiv=1, location=(0, 0, 0.28), scale=(1, 0.9, 0.6), mat=mat_stone)
    displace_verts(body, 0.12, 21)
    parent_to_root(root, body)
    return root


def make_cave_entrance():
    mat_stone = make_material("mat_cave", PIEDRA_OSCURA)
    mat_dark = make_material("mat_cave_dark", METAL_OSCURO)
    root = make_root("rock_cave_entrance")
    left = make_cube("left", scale=(0.5, 1.4, 0.8), location=(-0.55, 0, 0.7), mat=mat_stone)
    right = make_cube("right", scale=(0.5, 1.4, 0.8), location=(0.55, 0, 0.7), mat=mat_stone)
    lintel = make_cube("lintel", scale=(1.4, 0.4, 0.8), location=(0, 0, 1.45), mat=mat_stone)
    void = make_cube("void", scale=(0.7, 1.0, 0.5), location=(0, 0, 0.55), mat=mat_dark)
    parent_to_root(root, left, right, lintel, void)
    return root


def make_crystal(color_hex: str, name: str):
    mat = make_material(f"mat_crystal_{name}", color_hex, roughness=0.15, metallic=0.2)
    root = make_root(name)
    shard = make_cone("shard", 0.18, 0.0, 0.55, segments=6, location=(0, 0, 0.28), mat=mat)
    parent_to_root(root, shard)
    return root


def make_fence(wood=True):
    mat = make_material("mat_fence_wood" if wood else "mat_fence_stone", MADERA_CLARA if wood else PIEDRA)
    name = "prop_fence_wood" if wood else "prop_fence_stone"
    root = make_root(name)
    for x in (-0.45, 0.45):
        post = make_cube(f"post_{x}", scale=(0.08, 0.55, 0.08), location=(x, 0, 0.275), mat=mat)
        parent_keep_transform(post, root)
    rail = make_cube("rail", scale=(1.0, 0.06, 0.06), location=(0, 0, 0.45), mat=mat)
    parent_keep_transform(rail, root)
    return root


def make_archway():
    mat = make_material("mat_arch", PIEDRA)
    root = make_root("prop_archway")
    for x in (-0.7, 0.7):
        pillar = make_cube(f"pillar_{x}", scale=(0.35, 1.6, 0.35), location=(x, 0, 0.8), mat=mat)
        parent_keep_transform(pillar, root)
    top = make_cube("top", scale=(1.6, 0.35, 0.35), location=(0, 0, 1.55), mat=mat)
    parent_keep_transform(top, root)
    return root


def make_bridge():
    mat_wood = make_material("mat_bridge", MADERA_CLARA)
    root = make_root("prop_bridge")
    deck = make_cube("deck", scale=(2.8, 0.12, 1.0), location=(0, 0, 0.35), mat=mat_wood)
    for x in (-1.1, 1.1):
        rail = make_cube(f"rail_{x}", scale=(2.6, 0.35, 0.06), location=(0, 0, 0.6), mat=mat_wood)
        parent_keep_transform(rail, root)
    parent_to_root(root, deck)
    return root


def make_well():
    mat_stone = make_material("mat_well_stone", PIEDRA)
    mat_roof = make_material("mat_well_roof", MADERA_OSCURA)
    root = make_root("prop_well")
    base = make_cylinder("base", 0.42, 0.6, segments=8, location=(0, 0, 0.3), mat=mat_stone)
    roof = make_cone("roof", 0.55, 0.0, 0.35, segments=8, location=(0, 0, 1.05), mat=mat_roof)
    post = make_cylinder("post", 0.05, 0.9, segments=4, location=(0.35, 0, 0.75), mat=mat_roof)
    parent_to_root(root, base, roof, post)
    return root


def make_stall():
    mat_wood = make_material("mat_stall_wood", MADERA_CLARA)
    mat_awning = make_material("mat_stall_awning", ROJO_ACENTO)
    root = make_root("prop_stall")
    counter = make_cube("counter", scale=(1.4, 0.5, 0.7), location=(0, 0, 0.25), mat=mat_wood)
    awning = make_cube("awning", scale=(1.6, 0.08, 1.0), location=(0, 0, 0.85), rotation=(math.radians(12), 0, 0), mat=mat_awning)
    for x in (-0.6, 0.6):
        leg = make_cube(f"leg_{x}", scale=(0.08, 0.5, 0.08), location=(x, 0, 0.25), mat=mat_wood)
        parent_keep_transform(leg, root)
    parent_to_root(root, counter, awning)
    return root


def make_pot_cooking():
    mat_metal = make_material("mat_pot", METAL_GRIS)
    mat_fire = make_material("mat_pot_fire", NARANJA_ACENTO, roughness=0.3)
    root = make_root("prop_pot_cooking")
    pot = make_cylinder("pot", 0.28, 0.25, segments=8, location=(0, 0, 0.35), mat=mat_metal)
    flame = make_cone("flame", 0.12, 0.0, 0.2, segments=6, location=(0, 0, 0.12), mat=mat_fire)
    parent_keep_transform(flame, root)
    parent_to_root(root, pot)
    return root


def make_barn():
    mat_wall = make_material("mat_barn_wall", BARN_RED)
    mat_roof = make_material("mat_barn_roof", MADERA_OSCURA)
    mat_door = make_material("mat_barn_door", MADERA_CLARA)
    root = make_root("building_barn")
    body = make_cube("body", scale=(5.0, 3.2, 4.0), location=(0, 0, 1.6), mat=mat_wall)
    roof = make_cube("roof", scale=(5.4, 1.2, 4.2), location=(0, 0, 3.6), mat=mat_roof)
    door = make_cube("door", scale=(1.4, 2.2, 0.15), location=(0, 0, 0), mat=mat_door)
    parent_local(door, body, (0, -2.05, -0.5))
    parent_to_root(root, body, roof)
    return root


def make_bed():
    mat_wood = make_material("mat_bed_wood", MADERA_CLARA)
    mat_blanket = make_material("mat_bed_blanket", HERO_BLUE)
    root = make_root("interior_bed")
    frame = make_cube("frame", scale=(1.4, 0.2, 0.9), location=(0, 0, 0.1), mat=mat_wood)
    mattress = make_cube("mattress", scale=(1.3, 0.15, 0.85), location=(0, 0, 0.25), mat=mat_blanket)
    parent_to_root(root, frame, mattress)
    return root


def make_table():
    mat = make_material("mat_table", MADERA_CLARA)
    root = make_root("interior_table")
    top = make_cube("top", scale=(1.2, 0.08, 0.8), location=(0, 0, 0.45), mat=mat)
    for x, z in [(-0.5, -0.3), (0.5, -0.3), (-0.5, 0.3), (0.5, 0.3)]:
        leg = make_cube(f"leg_{x}_{z}", scale=(0.08, 0.45, 0.08), location=(x, 0, 0.225), mat=mat)
        parent_keep_transform(leg, root)
    parent_to_root(root, top)
    return root


def make_chair():
    mat = make_material("mat_chair", MADERA_CLARA)
    root = make_root("interior_chair")
    seat = make_cube("seat", scale=(0.45, 0.06, 0.45), location=(0, 0, 0.28), mat=mat)
    back = make_cube("back", scale=(0.45, 0.35, 0.06), location=(0, -0.18, 0.5), mat=mat)
    for x in (-0.18, 0.18):
        leg = make_cube(f"leg_{x}", scale=(0.06, 0.28, 0.06), location=(x, 0.15, 0.14), mat=mat)
        parent_keep_transform(leg, root)
    parent_to_root(root, seat, back)
    return root


def make_book():
    mat = make_material("mat_book", MADERA_OSCURA)
    root = make_root("interior_book")
    cover = make_cube("cover", scale=(0.22, 0.28, 0.04), location=(0, 0, 0.14), mat=mat)
    parent_to_root(root, cover)
    return root


def make_scroll_item():
    mat = make_material("mat_scroll", FOG_WHITE)
    root = make_root("interior_scroll")
    roll = make_cylinder("roll", 0.05, 0.28, segments=6, location=(0, 0, 0.14), rotation=(0, math.pi / 2, 0), mat=mat)
    parent_to_root(root, roll)
    return root


def make_sword(blade_color: str, name: str):
    mat_blade = make_material(f"mat_blade_{name}", blade_color, metallic=0.6, roughness=0.35)
    mat_handle = make_material(f"mat_handle_{name}", MADERA_OSCURA)
    root = make_root(name)
    blade = make_cube("blade", scale=(0.06, 0.55, 0.12), location=(0, 0, 0.45), mat=mat_blade)
    guard = make_cube("guard", scale=(0.18, 0.04, 0.04), location=(0, 0, 0.16), mat=mat_blade)
    handle = make_cylinder("handle", 0.03, 0.14, segments=6, location=(0, 0, 0.07), mat=mat_handle)
    parent_to_root(root, blade, guard, handle)
    return root


def make_dagger():
    return make_sword(METAL_GRIS, "equip_dagger")


def make_spear():
    mat = make_material("mat_spear", MADERA_CLARA)
    mat_tip = make_material("mat_spear_tip", METAL_GRIS, metallic=0.5)
    root = make_root("equip_spear")
    shaft = make_cylinder("shaft", 0.03, 1.1, segments=6, location=(0, 0, 0.55), mat=mat)
    tip = make_cone("tip", 0.05, 0.0, 0.18, segments=6, location=(0, 0, 1.15), mat=mat_tip)
    parent_to_root(root, shaft, tip)
    return root


def make_bow():
    mat = make_material("mat_bow", MADERA_OSCURA)
    root = make_root("equip_bow")
    bow = make_torus("bow", 0.35, 0.04, major_segments=8, minor_segments=4, location=(0, 0, 0.35), mat=mat)
    bow.rotation_euler = (math.pi / 2, 0, 0)
    parent_to_root(root, bow)
    return root


def make_staff():
    mat = make_material("mat_staff", MADERA_OSCURA)
    mat_orb = make_material("mat_staff_orb", POTION_PURPLE, roughness=0.2)
    root = make_root("equip_staff")
    pole = make_cylinder("pole", 0.04, 1.0, segments=6, location=(0, 0, 0.5), mat=mat)
    orb = make_uv_sphere("orb", 0.08, segments=6, ring_count=3, location=(0, 0, 1.05), mat=mat_orb)
    parent_to_root(root, pole, orb)
    return root


def make_shield(metal=False):
    mat = make_material("mat_shield_metal" if metal else "mat_shield_wood", METAL_GRIS if metal else MADERA_CLARA, metallic=0.4 if metal else 0.0)
    name = "equip_shield_metal" if metal else "equip_shield_wood"
    root = make_root(name)
    body = make_cylinder("body", 0.35, 0.06, segments=8, location=(0, 0, 0.35), mat=mat)
    boss = make_uv_sphere("boss", 0.06, segments=6, ring_count=2, location=(0, 0, 0.38), mat=make_material("mat_boss", METAL_OSCURO, metallic=0.5))
    parent_to_root(root, body, boss)
    return root


def make_pickaxe():
    mat = make_material("mat_pick", MADERA_CLARA)
    mat_head = make_material("mat_pick_head", METAL_GRIS, metallic=0.5)
    root = make_root("equip_pickaxe")
    handle = make_cylinder("handle", 0.04, 0.7, segments=6, location=(0, 0, 0.35), mat=mat)
    head = make_cube("head", scale=(0.28, 0.06, 0.06), location=(0, 0, 0.68), rotation=(0, 0, math.pi / 4), mat=mat_head)
    parent_to_root(root, handle, head)
    return root


def make_axe():
    mat = make_material("mat_axe", MADERA_CLARA)
    mat_head = make_material("mat_axe_head", METAL_GRIS, metallic=0.5)
    root = make_root("equip_axe")
    handle = make_cylinder("handle", 0.04, 0.65, segments=6, location=(0, 0, 0.32), mat=mat)
    head = make_cube("head", scale=(0.2, 0.06, 0.12), location=(0.08, 0, 0.62), mat=mat_head)
    parent_to_root(root, handle, head)
    return root


def make_hammer():
    mat = make_material("mat_hammer", MADERA_CLARA)
    mat_head = make_material("mat_hammer_head", METAL_GRIS, metallic=0.5)
    root = make_root("equip_hammer")
    handle = make_cylinder("handle", 0.04, 0.55, segments=6, location=(0, 0, 0.27), mat=mat)
    head = make_cube("head", scale=(0.22, 0.1, 0.1), location=(0, 0, 0.58), mat=mat_head)
    parent_to_root(root, handle, head)
    return root


def make_hoe():
    mat = make_material("mat_hoe", MADERA_CLARA)
    mat_blade = make_material("mat_hoe_blade", METAL_GRIS, metallic=0.5)
    root = make_root("equip_hoe")
    handle = make_cylinder("handle", 0.04, 0.75, segments=6, location=(0, 0, 0.37), mat=mat)
    blade = make_cube("blade", scale=(0.22, 0.04, 0.1), location=(0.1, 0, 0.72), mat=mat_blade)
    parent_to_root(root, handle, blade)
    return root


def make_watering_can():
    mat = make_material("mat_can", METAL_GRIS, metallic=0.4)
    root = make_root("equip_watering_can")
    body = make_cube("body", scale=(0.28, 0.22, 0.18), location=(0, 0, 0.2), mat=mat)
    spout = make_cylinder("spout", 0.025, 0.2, segments=4, location=(0.18, 0, 0.28), rotation=(0, 0, math.pi / 2), mat=mat)
    parent_to_root(root, body, spout)
    return root


def make_potion(color_hex: str, name: str):
    mat = make_material(f"mat_potion_{name}", color_hex, roughness=0.15)
    root = make_root(name)
    bottle = make_cylinder("bottle", 0.1, 0.32, segments=8, location=(0, 0, 0.2), mat=mat)
    cork = make_cylinder("cork", 0.05, 0.05, segments=6, location=(0, 0, 0.38), mat=make_material(f"mat_cork_{name}", MADERA_OSCURA))
    parent_to_root(root, bottle, cork)
    return root


def make_food(kind: str, color_hex: str):
    mat = make_material(f"mat_food_{kind}", color_hex)
    root = make_root(kind)
    if "bread" in kind or "pie" in kind:
        mesh = make_cube("food", scale=(0.28, 0.1, 0.22) if "bread" in kind else (0.26, 0.12, 0.26), location=(0, 0, 0.08), mat=mat)
    elif "soup" in kind:
        mesh = make_cylinder("food", 0.16, 0.1, segments=8, location=(0, 0, 0.08), mat=mat)
    elif "meat" in kind:
        mesh = make_cube("food", scale=(0.24, 0.1, 0.16), location=(0, 0, 0.08), mat=mat)
    else:
        mesh = make_uv_sphere("food", 0.14, segments=6, ring_count=3, location=(0, 0, 0.14), mat=mat)
    parent_to_root(root, mesh)
    return root


def make_material_item(kind: str, color_hex: str, shape="cube"):
    mat = make_material(f"mat_item_{kind}", color_hex, metallic=0.3 if "ingot" in kind or "coin" in kind else 0.0)
    root = make_root(kind)
    if shape == "cylinder":
        mesh = make_cylinder("item", 0.12, 0.5 if "log" not in kind else 0.55, segments=8, location=(0, 0, 0.28), mat=mat)
        if "log" in kind:
            mesh.rotation_euler = (0, math.pi / 2, 0)
    elif shape == "sphere":
        mesh = make_uv_sphere("item", 0.14, segments=6, ring_count=3, location=(0, 0, 0.14), mat=mat)
    elif "ingot" in kind:
        mesh = make_cube("item", scale=(0.35, 0.12, 0.2), location=(0, 0, 0.06), mat=mat)
    elif "coin" in kind:
        mesh = make_cylinder("item", 0.16, 0.04, segments=12, location=(0, 0, 0.02), mat=mat)
    elif "key" in kind:
        mesh = make_torus("item", 0.08, 0.02, major_segments=6, minor_segments=4, location=(0, 0, 0.12), mat=mat)
    elif "scroll" in kind:
        mesh = make_cylinder("item", 0.05, 0.3, segments=6, location=(0, 0, 0.15), rotation=(0, math.pi / 2, 0), mat=mat)
    else:
        mesh = make_cube("item", scale=(0.22, 0.22, 0.22), location=(0, 0, 0.11), mat=mat)
    parent_to_root(root, mesh)
    return root


def make_character(kind: str, body_color: str, accent: str | None = None):
    mat_body = make_material(f"mat_char_body_{kind}", body_color)
    mat_head = make_material(f"mat_char_head_{kind}", FOG_WHITE)
    mat_hat = make_material(f"mat_char_hat_{kind}", accent or MADERA_OSCURA)
    mat_pack = make_material(f"mat_char_pack_{kind}", LEATHER_TAN)
    mat_boot = make_material(f"mat_char_boot_{kind}", MADERA_OSCURA)
    mat_scarf = make_material(f"mat_char_scarf_{kind}", NARANJA_ACENTO)
    mat_pompom = make_material(f"mat_pompom_{kind}", FOG_WHITE)

    root = make_root(kind)
    torso = make_cylinder("torso", 0.2, 0.52, segments=6, location=(0, 0, 0.62), mat=mat_body)
    scarf = make_cylinder("scarf", 0.12, 0.1, segments=6, location=(0, 0, 1.0), mat=mat_scarf)
    head = make_uv_sphere("head", 0.15, segments=6, ring_count=4, location=(0, 0, 1.22), mat=mat_head)
    hat_base = make_cylinder("hat_base", 0.17, 0.06, segments=8, location=(0, 0, 1.35), mat=mat_hat)
    hat_top = make_cone("hat_top", 0.13, 0.02, 0.22, segments=8, location=(0, 0, 1.47), mat=mat_hat)
    hat_pompom = make_uv_sphere("pompom", 0.04, segments=4, ring_count=2, location=(0, 0, 1.7), mat=mat_pompom)
    pack_body = make_cube("pack_body", scale=(0.3, 0.18, 0.38), location=(0, 0.22, 0.7), mat=mat_pack)
    pack_top = make_cube("pack_top", scale=(0.28, 0.16, 0.1), location=(0, 0.22, 0.94), mat=mat_pack)
    strap_l = make_cylinder("strap_l", 0.02, 0.45, segments=4, location=(-0.1, 0.12, 0.72), mat=mat_pack)
    strap_r = make_cylinder("strap_r", 0.02, 0.45, segments=4, location=(0.1, 0.12, 0.72), mat=mat_pack)
    leg_l = make_cylinder("leg_l", 0.07, 0.38, segments=6, location=(-0.1, 0, 0.19), mat=mat_body)
    leg_r = make_cylinder("leg_r", 0.07, 0.38, segments=6, location=(0.1, 0, 0.19), mat=mat_body)
    boot_l = make_cube("boot_l", scale=(0.1, 0.18, 0.12), location=(-0.1, 0.02, 0.06), mat=mat_boot)
    boot_r = make_cube("boot_r", scale=(0.1, 0.18, 0.12), location=(0.1, 0.02, 0.06), mat=mat_boot)
    parent_to_root(
        root,
        torso,
        scarf,
        head,
        hat_base,
        hat_top,
        hat_pompom,
        pack_body,
        pack_top,
        strap_l,
        strap_r,
        leg_l,
        leg_r,
        boot_l,
        boot_r,
    )
    return root


def make_creature(kind: str, color_hex: str, blob=False):
    mat = make_material(f"mat_creature_{kind}", color_hex)
    root = make_root(kind)
    if blob:
        body = make_icosphere("body", 0.42, subdiv=1, location=(0, 0, 0.28), scale=(1, 0.8, 0.7), mat=mat)
    elif "wolf" in kind or "golem" in kind:
        body = make_cylinder("body", 0.35 if "wolf" in kind else 0.4, 0.7 if "wolf" in kind else 0.85, segments=8, location=(0, 0, 0.45), mat=mat)
    else:
        body = make_icosphere("body", 0.3, subdiv=1, location=(0, 0, 0.35), mat=mat)
    parent_to_root(root, body)
    return root


def make_ui_icon(kind: str, color_hex: str):
    mat = make_material(f"mat_ui_{kind}", color_hex)
    root = make_root(kind)
    icon = make_plane("icon", size=0.45, location=(0, 0, 0.01), rotation=(math.pi / 2, 0, 0), mat=mat)
    parent_to_root(root, icon)
    return root


def load_catalog_kinds() -> list[str]:
    with open(CATALOG_KINDS_PATH, encoding="utf-8") as f:
        return [line.strip() for line in f if line.strip() and not line.startswith("#")]


KIND_BUILDERS: dict[str, callable] = {
    # Terrain
    "terrain_grass": lambda: make_terrain_tile(VERDE_CLARO, "terrain_grass"),
    "terrain_dirt": lambda: make_terrain_tile(TIERRA, "terrain_dirt"),
    "terrain_stone": lambda: make_terrain_tile(PIEDRA, "terrain_stone"),
    "terrain_path_grass": lambda: make_terrain_tile(VERDE_BOSQUE, "terrain_path_grass", width=0.9),
    "terrain_path_dirt": lambda: make_terrain_tile(TIERRA, "terrain_path_dirt", width=0.9),
    "terrain_path_stone": lambda: make_terrain_tile(PIEDRA_OSCURA, "terrain_path_stone", width=0.9),
    "terrain_cliff": make_terrain_cliff,
    "terrain_slope": make_terrain_slope,
    "terrain_water": lambda: make_terrain_tile(NEON_CYAN, "terrain_water", height=0.08),
    # Flora trees
    "flora_tree_oak": make_tree_oak,
    "flora_tree_oak_autumn": make_tree_oak_autumn,
    "flora_tree_pine": make_tree_pine,
    "flora_tree_birch": make_tree_birch,
    "flora_tree_cherry": make_tree_cherry,
    "flora_tree_red": make_tree_red,
    "flora_shrub": make_shrub_small,
    "flora_bush": make_shrub_bush,
    "flora_flower_red": lambda: make_flower(ROJO_ACENTO, "flora_flower_red"),
    "flora_flower_yellow": lambda: make_flower(AMARILLO_ACENTO, "flora_flower_yellow"),
    "flora_flower_blue": lambda: make_flower(CERULEAN, "flora_flower_blue"),
    "flora_flower_white": lambda: make_flower(FOG_WHITE, "flora_flower_white"),
    "flora_grass_tuft": make_grass_tuft,
    "flora_mushroom_red": lambda: make_mushroom(ROJO_ACENTO, "flora_mushroom_red"),
    "flora_mushroom_brown": lambda: make_mushroom(MADERA_OSCURA, "flora_mushroom_brown"),
    # Rocks & crystals
    "rock_small": make_rock_small,
    "rock_medium": make_rock_medium,
    "rock_boulder": make_rock_large,
    "rock_cave_entrance": make_cave_entrance,
    "crystal_yellow": lambda: make_crystal(AMARILLO_ACENTO, "crystal_yellow"),
    "crystal_purple": lambda: make_crystal(POTION_PURPLE, "crystal_purple"),
    "crystal_blue": lambda: make_crystal(CERULEAN, "crystal_blue"),
    # Props
    "prop_fence_wood": lambda: make_fence(True),
    "prop_fence_stone": lambda: make_fence(False),
    "prop_pillar": make_stone_pillar,
    "prop_archway": make_archway,
    "prop_bridge": make_bridge,
    "prop_well": make_well,
    "machine_wood_extractor": make_machine_wood_extractor,
    "machine_stone_extractor": make_machine_stone_drill,
    "machine_launch_pad": make_launch_pad,
    "prop_stall": make_stall,
    "prop_barrel": make_barrel,
    "prop_crate": make_crate_wood,
    "prop_crate_large": make_storage_box_small,
    "prop_lantern": make_lantern,
    "prop_sign": make_sign,
    "prop_pot_cooking": make_pot_cooking,
    "building_cottage": make_cabin_small,
    "building_barn": make_barn,
    # Interiors
    "interior_bed": make_bed,
    "interior_table": make_table,
    "interior_chair": make_chair,
    "interior_book": make_book,
    "interior_scroll": make_scroll_item,
    # Equipment
    "equip_sword_iron": lambda: make_sword(METAL_GRIS, "equip_sword_iron"),
    "equip_sword_steel": lambda: make_sword(PIEDRA, "equip_sword_steel"),
    "equip_sword_gold": lambda: make_sword(AMARILLO_ACENTO, "equip_sword_gold"),
    "equip_dagger": make_dagger,
    "equip_spear": make_spear,
    "equip_bow": make_bow,
    "equip_staff": make_staff,
    "equip_shield_wood": lambda: make_shield(False),
    "equip_shield_metal": lambda: make_shield(True),
    "equip_pickaxe": make_pickaxe,
    "equip_axe": make_axe,
    "equip_hammer": make_hammer,
    "equip_hoe": make_hoe,
    "equip_watering_can": make_watering_can,
    # Consumables
    "item_potion_red": lambda: make_potion(ROJO_ACENTO, "item_potion_red"),
    "item_potion_blue": lambda: make_potion(POTION_BLUE, "item_potion_blue"),
    "item_potion_green": lambda: make_potion(POTION_GREEN, "item_potion_green"),
    "item_potion_purple": lambda: make_potion(POTION_PURPLE, "item_potion_purple"),
    "item_potion_yellow": lambda: make_potion(AMARILLO_ACENTO, "item_potion_yellow"),
    "item_food_apple": lambda: make_food("item_food_apple", ROJO_ACENTO),
    "item_food_orange": lambda: make_food("item_food_orange", NARANJA_ACENTO),
    "item_food_grape": lambda: make_food("item_food_grape", NEON_PINK),
    "item_food_bread": lambda: make_food("item_food_bread", AMARILLO_ACENTO),
    "item_food_pie": lambda: make_food("item_food_pie", MADERA_CLARA),
    "item_food_meat": lambda: make_food("item_food_meat", MADERA_OSCURA),
    "item_food_soup": lambda: make_food("item_food_soup", POTION_GREEN),
    # Materials
    "item_mat_log": make_wood_log,
    "item_mat_sticks": lambda: make_material_item("item_mat_sticks", MADERA_CLARA),
    "item_mat_stone": lambda: make_material_item("item_mat_stone", PIEDRA),
    "item_mat_ingot_iron": lambda: make_material_item("item_mat_ingot_iron", METAL_GRIS),
    "item_mat_ingot_gold": lambda: make_material_item("item_mat_ingot_gold", AMARILLO_ACENTO),
    "item_mat_leather": lambda: make_material_item("item_mat_leather", LEATHER_TAN),
    "item_mat_gem_red": lambda: make_material_item("item_mat_gem_red", ROJO_ACENTO, "sphere"),
    "item_mat_gem_blue": lambda: make_material_item("item_mat_gem_blue", CERULEAN, "sphere"),
    "item_key": lambda: make_material_item("item_key", AMARILLO_ACENTO),
    "item_scroll_map": lambda: make_material_item("item_scroll_map", FOG_WHITE),
    "item_coin": lambda: make_material_item("item_coin", AMARILLO_ACENTO),
    # Characters & creatures
    "character_adventurer": lambda: make_character("character_adventurer", HERO_BLUE, FOG_WHITE),
    "character_farmer": lambda: make_character("character_farmer", FARMER_GREEN, FOG_WHITE),
    "character_elder": lambda: make_character("character_elder", ELDER_GRAY, FOG_WHITE),
    "character_guard": lambda: make_character("character_guard", GUARD_RED, METAL_GRIS),
    "creature_slime": lambda: make_creature("creature_slime", SLIME_GREEN, blob=True),
    "creature_plant": lambda: make_creature("creature_plant", VERDE_BOSQUE),
    "creature_wolf": lambda: make_creature("creature_wolf", METAL_OSCURO),
    "creature_golem": lambda: make_creature("creature_golem", GOLEM_STONE),
    "creature_spirit": lambda: make_creature("creature_spirit", SPIRIT_LAVENDER),
    # UI icons
    "ui_heart": lambda: make_ui_icon("ui_heart", ROJO_ACENTO),
    "ui_mana": lambda: make_ui_icon("ui_mana", POTION_BLUE),
    "ui_slot_sword": lambda: make_ui_icon("ui_slot_sword", METAL_GRIS),
    "ui_slot_shield": lambda: make_ui_icon("ui_slot_shield", MADERA_CLARA),
    "ui_slot_fire": lambda: make_ui_icon("ui_slot_fire", NARANJA_ACENTO),
    "ui_icon_inventory": lambda: make_ui_icon("ui_icon_inventory", MADERA_CLARA),
    "ui_icon_quest": lambda: make_ui_icon("ui_icon_quest", AMARILLO_ACENTO),
    "ui_icon_map": lambda: make_ui_icon("ui_icon_map", CERULEAN),
    "ui_icon_settings": lambda: make_ui_icon("ui_icon_settings", METAL_GRIS),
    "ui_icon_chat": lambda: make_ui_icon("ui_icon_chat", VERDE_CLARO),
    # Staging-only (space / future content)
    "machine_pump": make_machine_pump,
    "habitation_dome": make_habitation_dome,
    "solar_panel": make_solar_panel,
    "rocket_module_cabin": make_rocket_cabin,
    "rocket_module_fuel": make_rocket_fuel,
    "rocket_module_engine": make_rocket_engine,
    "rocket_module_cargo": make_rocket_cargo,
    "landing_strut": make_landing_strut,
    "moon_boulder": make_moon_boulder,
    "moon_crater_rim": make_moon_crater,
    "solar_array_panel": make_solar_array,
    "comms_dish": make_comms_dish,
    "lunar_rover": make_lunar_rover,
    "prop_campfire": make_campfire,
    "flora_tree_birch": make_tree_birch,
}


def build_asset_registry() -> list[tuple[str, callable, bool]]:
    registry: list[tuple[str, callable, bool]] = []
    catalog = load_catalog_kinds()
    missing = [k for k in catalog if k not in KIND_BUILDERS]
    if missing:
        raise KeyError(f"Missing builders for catalog kinds: {', '.join(missing)}")
    for kind in catalog:
        registry.append((kind, KIND_BUILDERS[kind], False))
    for kind in sorted(STAGING_ONLY_KINDS):
        if kind not in catalog:
            registry.append((kind, KIND_BUILDERS[kind], True))
    return registry


ASSET_REGISTRY = build_asset_registry()


def main():
    opts = parse_cli_args()

    if opts.capture:
        print("=" * 60)
        print(f"CAPTURA overrides → {opts.capture}")
        run_capture_mode(opts.capture)
        print("=" * 60)
        return

    registry = ASSET_REGISTRY
    if opts.only:
        registry = [e for e in ASSET_REGISTRY if e[0] in opts.only]
        if not registry:
            print(f"ERROR: --only no coincide con ningún kind: {opts.only}")
            sys.exit(1)

    print("=" * 60)
    print("AGENCIA ESPACIAL SURVIVAL — Asset Generator v2.1")
    print("Cobertura: catalog-kinds.txt + staging + manual/")
    if opts.force:
        print("Modo: --force (ignora manual/*.blend y overrides JSON)")
    if opts.only:
        print(f"Filtro: --only {', '.join(sorted(opts.only))}")
    print("NOTA: Script de herramienta externa. Ver comentario al inicio.")
    print(f"Blender {bpy.app.version_string}")
    print(f"Game output: {OUTPUT_DIR}")
    print(f"Staging:     {STAGING_DIR}")
    print(f"Manual:      {MANUAL_DIR}")
    print("=" * 60)

    results = []
    errors = []
    warnings = []
    manual_count = 0
    override_count = 0

    for i, (kind, fn, staging_only) in enumerate(registry):
        out_label = f"{kind}{' (staging)' if staging_only else ''}"
        print(f"[{i + 1}/{len(registry)}] {out_label}...", end=" ")
        try:
            if manual_blend_path(kind) and not opts.force:
                manual_count += 1
            elif os.path.isfile(override_json_path(kind)) and not opts.force:
                override_count += 1

            stats = generate_asset_kind(kind, fn, staging_only, opts)
            results.append(stats)
            tri = stats["triangles"]
            src = stats.get("source", "")
            src_tag = f" [{src}]" if src and src != "procedural" else ""
            flag = " ✓" if tri <= MAX_TRIS_WARN else f" ⚠ {tri} tris (>{MAX_TRIS_WARN})"
            if tri > MAX_TRIS_WARN:
                warnings.append((kind, tri))
            print(f"{tri} tris{flag}{src_tag}")
        except Exception as e:
            errors.append((kind, str(e)))
            print(f"ERROR: {e}")
            traceback.print_exc()

    print("\n" + "=" * 60)
    print(f"COMPLETADO: {len(results)}/{len(registry)} assets generados")
    if manual_count:
        print(f"Desde manual/*.blend: {manual_count} (usa --force para regenerar procedural)")
    if override_count:
        print(f"Con overrides JSON:   {override_count} (tools/blender/manual/overrides/)")
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
