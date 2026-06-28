import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import { GLTFLoader } from "three/examples/jsm/loaders/GLTFLoader.js";
import type { MeshStats } from "./types";
import { fileUrl } from "./api";

export type ViewerOptions = {
  canvas: HTMLCanvasElement;
  onStats: (stats: MeshStats | null) => void;
  onStatus: (msg: string) => void;
  onError?: (msg: string) => void;
};

const GRID_SIZE = 20;

export class AssetViewer {
  private renderer: THREE.WebGLRenderer;
  private scene: THREE.Scene;
  private camera: THREE.PerspectiveCamera;
  private controls: OrbitControls;
  private loader = new GLTFLoader();
  private root = new THREE.Group();
  private grid: THREE.GridHelper;
  private axes: THREE.AxesHelper;
  private hemi: THREE.HemisphereLight;
  private sun: THREE.DirectionalLight;
  private wireframe = false;
  private autoRotate = false;
  private animId = 0;
  private current: THREE.Object3D | null = null;
  private onStats: (stats: MeshStats | null) => void;
  private onStatus: (msg: string) => void;
  private onError: (msg: string) => void;

  constructor(opts: ViewerOptions) {
    this.onStats = opts.onStats;
    this.onStatus = opts.onStatus;
    this.onError = opts.onError ?? (() => undefined);

    this.renderer = new THREE.WebGLRenderer({
      canvas: opts.canvas,
      antialias: true,
      alpha: false,
      powerPreference: "high-performance",
    });
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    this.renderer.outputColorSpace = THREE.SRGBColorSpace;
    this.renderer.toneMapping = THREE.ACESFilmicToneMapping;
    this.renderer.toneMappingExposure = 1.1;

    this.scene = new THREE.Scene();
    this.scene.background = new THREE.Color(0x0d1118);
    this.scene.add(this.root);

    this.grid = new THREE.GridHelper(GRID_SIZE, GRID_SIZE, 0x2a3a55, 0x1a2435);
    this.scene.add(this.grid);
    this.axes = new THREE.AxesHelper(2);
    this.scene.add(this.axes);

    this.hemi = new THREE.HemisphereLight(0xb8d4ff, 0x2a1f14, 0.55);
    this.scene.add(this.hemi);
    this.sun = new THREE.DirectionalLight(0xfff0d8, 1.15);
    this.sun.position.set(6, 10, 4);
    this.scene.add(this.sun);

    const fill = new THREE.DirectionalLight(0x8899bb, 0.35);
    fill.position.set(-4, 2, -6);
    this.scene.add(fill);

    this.camera = new THREE.PerspectiveCamera(45, 1, 0.01, 500);
    this.camera.position.set(4, 3, 5);

    this.controls = new OrbitControls(this.camera, opts.canvas);
    this.controls.enableDamping = true;
    this.controls.dampingFactor = 0.08;
    this.controls.target.set(0, 0.8, 0);

    this.resize();
    window.addEventListener("resize", () => this.resize());
    this.loop();
  }

  setWireframe(on: boolean): void {
    this.wireframe = on;
    if (this.current) this.applyWireframe(this.current);
  }

  setAutoRotate(on: boolean): void {
    this.autoRotate = on;
    this.controls.autoRotate = on;
    this.controls.autoRotateSpeed = 1.2;
  }

  resetCamera(): void {
    this.controls.reset();
    this.frameObject(this.current);
  }

  async loadGlb(rel: string): Promise<void> {
    this.clear();
    this.onStatus(`Cargando ${rel}…`);
    this.onStats(null);
    try {
      const gltf = await this.loader.loadAsync(fileUrl(rel));
      this.current = gltf.scene;
      this.root.add(gltf.scene);
      this.applyWireframe(gltf.scene);
      this.frameObject(gltf.scene);
      this.onStats(computeStats(gltf.scene));
      this.onStatus(`Listo: ${rel}`);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      this.onStatus(`Error: ${msg}`);
      this.onError(msg);
      throw err;
    }
  }

  dispose(): void {
    cancelAnimationFrame(this.animId);
    this.clear();
    this.renderer.dispose();
    this.controls.dispose();
  }

  private loop = (): void => {
    this.animId = requestAnimationFrame(this.loop);
    this.controls.update();
    this.renderer.render(this.scene, this.camera);
  };

  private resize(): void {
    const parent = this.renderer.domElement.parentElement;
    if (!parent) return;
    const w = parent.clientWidth;
    const h = parent.clientHeight;
    this.renderer.setSize(w, h, false);
    this.camera.aspect = w / Math.max(h, 1);
    this.camera.updateProjectionMatrix();
  }

  private clear(): void {
    if (this.current) {
      this.root.remove(this.current);
      disposeObject(this.current);
      this.current = null;
    }
  }

  private applyWireframe(obj: THREE.Object3D): void {
    obj.traverse((child) => {
      if (!(child instanceof THREE.Mesh)) return;
      child.frustumCulled = false;
      const mats = Array.isArray(child.material) ? child.material : [child.material];
      for (const m of mats) {
        if (!m) continue;
        m.wireframe = this.wireframe;
        m.side = THREE.DoubleSide;
      }
    });
  }

  private frameObject(obj: THREE.Object3D | null): void {
    if (!obj) {
      this.controls.target.set(0, 0.8, 0);
      this.camera.position.set(4, 3, 5);
      this.controls.update();
      return;
    }
    const box = new THREE.Box3().setFromObject(obj);
    const size = box.getSize(new THREE.Vector3());
    const center = box.getCenter(new THREE.Vector3());
    const maxDim = Math.max(size.x, size.y, size.z, 0.5);
    const dist = maxDim * 2.2;
    this.controls.target.copy(center);
    this.camera.position.set(center.x + dist * 0.7, center.y + dist * 0.45, center.z + dist * 0.7);
    this.camera.near = maxDim / 200;
    this.camera.far = maxDim * 50;
    this.camera.updateProjectionMatrix();
    this.controls.update();
  }
}

function computeStats(root: THREE.Object3D): MeshStats {
  let triangles = 0;
  let vertices = 0;
  let meshes = 0;
  const materials = new Set<THREE.Material>();
  const box = new THREE.Box3().setFromObject(root);
  const size = box.getSize(new THREE.Vector3());

  root.traverse((child) => {
    if (!(child instanceof THREE.Mesh)) return;
    meshes += 1;
    const geo = child.geometry;
    const idx = geo.index;
    const pos = geo.getAttribute("position");
    if (pos) vertices += pos.count;
    if (idx) triangles += idx.count / 3;
    else if (pos) triangles += pos.count / 3;
    const mats = Array.isArray(child.material) ? child.material : [child.material];
    for (const m of mats) if (m) materials.add(m);
  });

  return {
    triangles: Math.round(triangles),
    vertices,
    meshes,
    materials: materials.size,
    bbox: { x: size.x, y: size.y, z: size.z },
  };
}

function disposeObject(obj: THREE.Object3D): void {
  obj.traverse((child) => {
    if (!(child instanceof THREE.Mesh)) return;
    child.geometry.dispose();
    const mats = Array.isArray(child.material) ? child.material : [child.material];
    for (const m of mats) m?.dispose();
  });
}
