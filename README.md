# Cloudbate DL
Descargador individual de perfiles publicos de Cloudbate, en Rust para Windows.

Abre `abrir.cmd` (o `cloudbate-dl.exe`) y pega un perfil como XXX. Puedes agregar varios enlaces. Usa Listar para consultar y Descargar para guardar; el progreso y hashes se conservan en SQLite dentro de downloads. Los archivos quedan bajo la carpeta correspondiente al modelo. Los enlaces temporales se renuevan antes de descargar; los archivos completos se omiten. Actualizar consulta novedades y conserva el catalogo conocido.

Se elige la mayor calidad MP4 declarada por el reproductor. Los servidores que no responden quedan como errores reintentables, nunca como archivos completados. No se sustituyen videos por miniaturas. No requiere otro descargador instalado.

## Compilar
Instala Rust estable y las herramientas de compilacion MSVC para Windows.

```powershell
cargo build --release --locked
.\actualizar.cmd
```

El unico binario de distribucion es `cloudbate-dl.exe`. El instalador preserva datos y no reemplaza un ejecutable abierto. El repositorio contiene todas sus fuentes y Cargo.lock, sin dependencias de repositorios vecinos.

## Verificar
```powershell
cargo test --locked --offline
cargo fmt --check
cargo clippy --locked --offline --all-targets --all-features -- -D warnings
powershell -File tools/verify-standalone.ps1
```

Las pruebas de red son explicitas e ignoradas en la ejecucion ordinaria. No publiques downloads, bases de datos, cookies, perfiles de navegador ni ejecutables. Consulta ENGINE_MAINTENANCE.md para sincronizar mejoras comunes.

## Licencia y uso responsable

El código propio se distribuye bajo la [licencia MIT](LICENSE). El propósito educativo y las responsabilidades de uso se describen en [USAGE_NOTICE.md](USAGE_NOTICE.md). Esta licencia no concede derechos sobre contenido ajeno.
