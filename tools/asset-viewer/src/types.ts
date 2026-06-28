export type AssetEntry = {
  path: string;
  rel: string;
  name: string;
  ext: string;
  size: number;
  mtimeMs: number;
  category: string;
  inCatalog: boolean | null;
  catalogKind: string | null;
};

export type AssetListResponse = {
  repoRoot: string;
  assets: AssetEntry[];
};

export type MeshStats = {
  triangles: number;
  vertices: number;
  meshes: number;
  materials: number;
  bbox: { x: number; y: number; z: number };
};
