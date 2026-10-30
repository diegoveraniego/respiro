<div align="center">
  <img src="data/icons/hicolor/scalable/apps/org.diego.Respiro.svg" width="128" height="128" alt="Icono de Respiro" />
  <h1>Respiro</h1>
  <p><strong>Guía visual minimalista de respiración y meditación para GNOME</strong></p>
</div>

---

**Respiro** (`org.diego.Respiro`) es una aplicación nativa y minimalista para el escritorio GNOME escrita en **Rust** con **GTK4** y **Libadwaita**. Guía tus sesiones de respiración mediante una geometría orgánica animada sincronizada con la tasa de refresco de tu pantalla.

## ✨ Características

- **Geometría adaptable y orgánica**: El recorrido visual cambia automáticamente de forma según las fases activas del patrón (lente de 2 fases, triángulo de 3 fases o cuadrado curvado de 4 fases).
- **Patrones predefinidos y modo personalizado**:
  - `5-5-5-5` y `4-4-4-4` (*Box Breathing* / respiración cuadrada)
  - `4-7-8` (relajación profunda)
  - `5-5` (respiración coherente)
  - **Personalizado**: configura libremente la duración (hasta 30 s) de cada fase (*Inhala*, *Mantiene 1*, *Exhala*, *Mantiene 2*).
- **Temporizador opcional sin metas intrusivas**: Por defecto inicia **Sin límite** para no interrumpir ejercicios de regulación abiertos, con opción de elegir sesiones de `2`, `5`, `10` o `15 min` desde la configuración.
- **Sintetizador de ruido marrón filtrado (opcional)**: Generador en tiempo real de *Brown Noise* estereofónico pasado por un filtro pasa-banda resonante (SVF) cuya frecuencia de corte y factor $Q$ suben suavemente al inhalar, se sostienen al mantener y descienden al exhalar, como el oleaje o la propia respiración.
- **Interfaz libre de distracciones**:
  - Icono sutil de estado en pausa en la parte inferior central que se desvanece suavemente al reproducir.
  - Auto-ocultación de los botones superiores y del cursor tras 2.5 s de inactividad durante la sesión.
  - Adopta automáticamente el color de acento de GNOME (`@accent_bg_color`) y el tema claro/oscuro.
  - **Inhibición de inactividad**: evita que la pantalla se apague mientras una sesión está activa.
  - Guarda tu último patrón y preferencia de sonido en `~/.config/respiro/state.conf`.

## ⌨️ Controles y atajos

| Acción | Control / Atajo |
| :--- | :--- |
| **Iniciar / Pausar** | Clic en cualquier parte del lienzo o <kbd>Espacio</kbd> |
| **Reiniciar ciclo** | <kbd>R</kbd> o <kbd>Ctrl</kbd> + <kbd>R</kbd> |
| **Configuración** | Botón de engranaje en la esquina superior derecha |
| **Cerrar aplicación** | <kbd>Esc</kbd> o <kbd>Ctrl</kbd> + <kbd>Q</kbd> |

## 🚀 Instalación

### Requisitos previos

Asegúrate de tener instalado el toolchain de **Rust** (`cargo`) y las librerías de desarrollo de **GTK4** y **Libadwaita**:

- **Fedora**:
  ```bash
  sudo dnf install rust cargo gtk4-devel libadwaita-devel
  ```
- **Ubuntu / Debian**:
  ```bash
  sudo apt install cargo libgtk-4-dev libadwaita-1-dev
  ```
- **Arch Linux**:
  ```bash
  sudo pacman -S rust gtk4 libadwaita
  ```

### Instalación local (con lanzador e icono de escritorio)

Ejecuta el script incluido para compilar en modo `release` e instalar el binario, el archivo `.desktop` y el icono en `~/.local`:

```bash
./scripts/install-local.sh
```

También puedes ejecutarla directamente con Cargo durante el desarrollo:

```bash
cargo run --release
```

## Respiración ASCII en terminal / TTY

> 💡 **Repositorio standalone disponible**: Si solo buscas la versión de terminal sin el árbol del proyecto GTK4/GNOME, también existe como repositorio independiente en [**diegoveraniego/respiro-tty**](https://github.com/diegoveraniego/respiro-tty) (*"Meditation from your terminal — because touching grass isn't POSIX-compliant."*).

`respiro-cli` funciona en Linux, tanto en una consola TTY como en una terminal
gráfica con soporte ANSI. Se compila de forma independiente: **no necesita GTK,
GNOME, servidor gráfico ni audio**. Por defecto su interfaz está en **inglés** y
puedes activar la traducción al **español** con `--lang es` (o `-l es`, o la
variable de entorno `RESPIRO_LANG=es`).

```bash
./scripts/install-cli.sh
# Incluye ~/.local/bin en tu PATH si aún no está.
respiro-cli
```

También puedes compilar y ejecutar dentro del proyecto:

```bash
cargo build --release --no-default-features --features cli --bin respiro-cli
./target/release/respiro-cli
```

Durante la sesión solo se muestra la fase y su avance visual, sin segundos,
números ni cuenta atrás. Arranca automáticamente y continúa hasta salir.

- `orb`: el contorno se expande al inhalar, queda quieto en las pausas y se
  contrae al exhalar. La barra inferior muestra el avance de cada fase,
  incluidas las pausas.
- `path`: sigue `@` por una lente, triángulo o cuadrado según haya dos, tres o
  cuatro fases activas. Cada tramo representa una fase y avanza a su ritmo.
- `--compact`: reemplaza la figura por una barra de tres líneas en la terminal
  actual, sin entrar en pantalla completa.
- `--lang es` / `-l es`: cambia los textos de la sesión y la ayuda al español
  (por defecto `en`).
- `--no-color`: desactiva los colores ANSI por fase (también respeta `NO_COLOR`).
- `--bell` / `-b`: emite campana de terminal (`\a`) al cambiar de fase.
- `--auto-hide`: auto-oculta la barra inferior de controles tras unos segundos de
  inactividad (también configurable de forma persistente con `auto_hide = true` y
  `auto_hide_secs = 3.5` en `~/.config/respiro-tty/config.conf` o `~/.config/respiro/cli.conf`).
- **Espacio** pausa/reanuda; **r** reinicia la sesión; **a** / **Tab** alterna entre
  `orb` y `path` en vivo; **p** o **1**–**4** cambia el patrón en vivo; **q**, **Esc**
  o **Ctrl+C** salen y restauran la terminal. La pantalla completa se adapta al tamaño.

```bash
respiro-cli --pattern 4-7-8 --animation path
respiro-cli --pattern coherent --compact
respiro-cli --lang es --pattern 4-0-6-0 --cycles 3
respiro-cli --pattern 2.5,1,5,0
respiro-cli --list-patterns
respiro-cli --help
```

El patrón inicial es `5-5-5-5`. También están disponibles `4-4-4-4` (`box`),
`4-7-8` (`relax`) y `5-5` (`coherent`). Los tiempos personalizados se separan
por guiones o comas: dos valores significan inhalar/exhalar; tres,
inhalar/mantener/exhalar; cuatro, inhalar/mantener/exhalar/descansar.
Inhalar y exhalar aceptan de 1 a 30 segundos, y las pausas de 0 a 30;
cero omite esa pausa. Los tiempos solo se escriben al configurar el comando.
`--cycles N` termina tras N ciclos completos; el tiempo en pausa no cuenta.
Las opciones se aplican a esa ejecución; puedes guardar tu combinación en un
alias del shell. La configuración de la aplicación GNOME es independiente.

### Fastfetch

`--snapshot` genera un fotograma ASCII estático, sin códigos de control, y termina
inmediatamente. Sirve como logo de Fastfetch y también para guardar en un archivo:

```bash
fastfetch --logo-type command-raw --logo 'respiro-cli --snapshot --animation path --pattern 4-7-8'
respiro-cli --snapshot > /tmp/respiro-logo.txt
fastfetch --logo-type file-raw --logo /tmp/respiro-logo.txt
```

Para añadirlo a tu configuración de Fastfetch, incorpora esta propiedad `logo`
al objeto JSONC existente (requiere una versión con `command-raw`):

```jsonc
"logo": {
  "type": "command-raw",
  "source": "respiro-cli --snapshot --animation path --pattern 4-7-8"
}
```

La integración de logo es estática. Para tener una sesión animada después de
mostrar Fastfetch, ejecuta `fastfetch && respiro-cli --compact --cycles 3`.
No uses una sesión animada como fuente del logo: necesita entrada interactiva.

### Comprobaciones de la CLI

```bash
cargo test --no-default-features --features cli --bin respiro-cli
cargo build --no-default-features --features cli --bin respiro-cli
python3 tests/cli_pty.py
```

## 📄 Licencia

Distribuido bajo la licencia **MIT**.
