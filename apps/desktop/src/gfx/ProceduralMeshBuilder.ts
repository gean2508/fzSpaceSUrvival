import * as THREE from "three";
import { PALETTE } from "../game/art/palette";

/**
 * Procedural, code-only art. No .gltf/.obj assets: meshes are built once from
 * BufferGeometry vertex arrays and cached. Materials extend MeshToonMaterial
 * via onBeforeCompile to quantize outgoing light into 3 hard cel bands, fed by
 * a 1×4 canvas gradient. Outlines use inverted-hull backface shells (mobile-safe).
 */

// ---- 3-tone cel gradient (1×4) -----------------------------------------
let celRamp: THREE.CanvasTexture | null = null;

function makeCelRamp(): THREE.CanvasTexture {
  if (celRamp) return celRamp;
  const c = document.createElement("canvas");
  c.width = 4;
  c.height = 1;
  const ctx = c.getContext("2d")!;
  const bands = ["#3a4a63", "#7c93a6", "#cfe0e8", "#ffffff"];
  for (let i = 0; i < bands.length; i++) {
    ctx.fillStyle = bands[i];
    ctx.fillRect(i, 0, 1, 1);
  }
  celRamp = new THREE.CanvasTexture(c);
  celRamp.minFilter = THREE.NearestFilter;
  celRamp.magFilter = THREE.NearestFilter;
  celRamp.generateMipmaps = false;
  celRamp.needsUpdate = true;
  return celRamp;
}

/** Toon material with 3-tone cel quantization injected at shader compile time. */
export function makeCelMaterial(color: THREE.ColorRepresentation): THREE.MeshToonMaterial {
  const mat = new THREE.MeshToonMaterial({ color, gradientMap: makeCelRamp() });
  mat.onBeforeCompile = (shader) => {
    // Crush outgoing light into 3 discrete bands before the final output pass.
    shader.fragmentShader = shader.fragmentShader.replace(
      "#include <output_fragment>",
      `
      outgoingLight = floor(outgoingLight * 3.0 + vec3(0.02)) / 3.0;
      #include <output_fragment>
      `
    );
  };
  return mat;
}

/** Inverted-hull outline mesh (cheap mobile-friendly ink line). */
export function makeOutlineShell(
  geometry: THREE.BufferGeometry,
  thickness = 0.06
): THREE.Mesh {
  const mat = new THREE.MeshBasicMaterial({ color: PALETTE.night, side: THREE.BackSide });
  const shell = new THREE.Mesh(geometry, mat);
  shell.scale.multiplyScalar(1 + thickness);
  shell.renderOrder = -1;
  return shell;
}

// ---- procedural box from vertex arrays ----------------------------------
const boxCache = new Map<string, THREE.BufferGeometry>();

/**
 * Build an axis-aligned box from explicit vertex/index arrays (no external assets).
 * Origin at geometric center; cached by dimensions.
 */
export function buildBoxGeometry(w: number, h: number, d: number): THREE.BufferGeometry {
  const key = `${w.toFixed(3)}|${h.toFixed(3)}|${d.toFixed(3)}`;
  const cached = boxCache.get(key);
  if (cached) return cached;

  const hw = w / 2;
  const hh = h / 2;
  const hd = d / 2;

  // 24 unique vertices (4 per face) with per-face normals for correct cel shading.
  const positions: number[] = [];
  const normals: number[] = [];
  const indices: number[] = [];

  const pushFace = (
    ax: number,
    ay: number,
    az: number,
    bx: number,
    by: number,
    bz: number,
    cx: number,
    cy: number,
    cz: number,
    dx: number,
    dy: number,
    dz: number,
    nx: number,
    ny: number,
    nz: number
  ) => {
    const base = positions.length / 3;
    positions.push(ax, ay, az, bx, by, bz, cx, cy, cz, dx, dy, dz);
    for (let i = 0; i < 4; i++) normals.push(nx, ny, nz);
    indices.push(base, base + 1, base + 2, base, base + 2, base + 3);
  };

  // +Y top
  pushFace(-hw, hh, -hd, hw, hh, -hd, hw, hh, hd, -hw, hh, hd, 0, 1, 0);
  // -Y bottom
  pushFace(-hw, -hh, hd, hw, -hh, hd, hw, -hh, -hd, -hw, -hh, -hd, 0, -1, 0);
  // +Z front
  pushFace(-hw, -hh, hd, hw, -hh, hd, hw, hh, hd, -hw, hh, hd, 0, 0, 1);
  // -Z back
  pushFace(hw, -hh, -hd, -hw, -hh, -hd, -hw, hh, -hd, hw, hh, -hd, 0, 0, -1);
  // +X right
  pushFace(hw, -hh, hd, hw, -hh, -hd, hw, hh, -hd, hw, hh, hd, 1, 0, 0);
  // -X left
  pushFace(-hw, -hh, -hd, -hw, -hh, hd, -hw, hh, hd, -hw, hh, -hd, -1, 0, 0);

  const geo = new THREE.BufferGeometry();
  geo.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
  geo.setAttribute("normal", new THREE.Float32BufferAttribute(normals, 3));
  geo.setIndex(indices);
  geo.computeBoundingSphere();
  boxCache.set(key, geo);
  return geo;
}

// ---- character mesh -----------------------------------------------------
const characterGeo = new THREE.CapsuleGeometry(0.34, 0.55, 6, 14);
const noseGeo = new THREE.ConeGeometry(0.14, 0.3, 10);

export function buildCharacter(color: THREE.ColorRepresentation): {
  group: THREE.Group;
  body: THREE.MeshToonMaterial;
} {
  const group = new THREE.Group();
  const body = makeCelMaterial(color);
  const bodyMesh = new THREE.Mesh(characterGeo, body);
  bodyMesh.castShadow = true;
  group.add(makeOutlineShell(characterGeo, 0.07));
  group.add(bodyMesh);

  const nose = new THREE.Mesh(noseGeo, makeCelMaterial(PALETTE.cream));
  nose.rotation.x = Math.PI / 2;
  nose.position.set(0, 0.12, 0.42);
  group.add(nose);
  return { group, body };
}

// ---- block / wall mesh --------------------------------------------------
export function buildBlock(
  halfW: number,
  height: number,
  halfD: number
): THREE.Group {
  const group = new THREE.Group();
  const geo = buildBoxGeometry(halfW * 2, height, halfD * 2);
  const climbable = height <= 3.0;
  const mesh = new THREE.Mesh(
    geo,
    makeCelMaterial(climbable ? PALETTE.earth : PALETTE.bark)
  );
  mesh.castShadow = true;
  mesh.receiveShadow = true;
  group.add(makeOutlineShell(geo, 0.03));
  group.add(mesh);
  return group;
}

// ---- interactable node mesh --------------------------------------------
const trunkGeo = new THREE.CylinderGeometry(0.18, 0.24, 1.1, 8);
const leafGeo = new THREE.IcosahedronGeometry(0.7, 0);
const rockGeo = new THREE.DodecahedronGeometry(0.5, 0);

export function buildInteractable(kind: string): THREE.Group {
  const group = new THREE.Group();
  if (kind === "tree") {
    const trunk = new THREE.Mesh(trunkGeo, makeCelMaterial(PALETTE.bark));
    trunk.position.y = 0.55;
    trunk.castShadow = true;
    const leaves = new THREE.Mesh(leafGeo, makeCelMaterial(PALETTE.meadowDeep));
    leaves.position.y = 1.4;
    leaves.castShadow = true;
    group.add(makeOutlineShell(leafGeo, 0.05));
    group.add(trunk, leaves);
  } else if (kind === "rock") {
    const rock = new THREE.Mesh(rockGeo, makeCelMaterial("#9aa3ad"));
    rock.position.y = 0.4;
    rock.castShadow = true;
    group.add(makeOutlineShell(rockGeo, 0.05));
    group.add(rock);
  } else {
    const crateGeo = buildBoxGeometry(0.8, 0.8, 0.8);
    const crate = new THREE.Mesh(crateGeo, makeCelMaterial("#b6894f"));
    crate.position.y = 0.4;
    crate.castShadow = true;
    group.add(makeOutlineShell(crateGeo, 0.04));
    group.add(crate);
  }
  return group;
}
