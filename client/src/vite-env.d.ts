/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_SERVER_HOST: string;
  readonly VITE_SERVER_PORT: string;
  readonly VITE_SERVER_HTTP_URL?: string;
  readonly VITE_APK_VERSION?: string;
  readonly VITE_IOS_URL?: string;
  readonly VITE_IOS_VERSION?: string;
  readonly VITE_LINUX_URL?: string;
  readonly VITE_LINUX_VERSION?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
