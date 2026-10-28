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
- **Guía sonora armónica opcional**: Tonos suaves de cuenco sintetizados en memoria al cambiar de fase para poder practicar con los ojos cerrados.
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

## 📄 Licencia

Distribuido bajo la licencia **MIT**.
