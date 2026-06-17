# AndroidManifest overlay — aplicar después de `npx cap add android`

En `android/app/src/main/AndroidManifest.xml`, dentro de `<application ...>`:

```xml
<application
    android:networkSecurityConfig="@xml/network_security_config"
    android:usesCleartextTraffic="true"
    ...>
```

Pasos:

1. `cd client && npm run build`
2. `npx cap add android` (solo la primera vez)
3. Copiar `android-overlay/res/xml/network_security_config.xml` → `android/app/src/main/res/xml/`
4. Editar `AndroidManifest.xml` como arriba
5. Build APK con `VITE_SERVER_HOST=192.168.x.x npm run build && npx cap sync`
