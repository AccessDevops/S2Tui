# Plan d'action multiplateforme S2Tui

## Analyse de l'état actuel

### ✅ Ce qui est déjà en place

| Composant | macOS | Windows | Linux |
|-----------|-------|---------|-------|
| Abstraction plateforme (`platform/`) | ✅ | ✅ | ✅ |
| Gestion permissions micro | ✅ | ✅ | ✅ |
| Configuration fenêtre overlay | ✅ | ✅ | ⚠️ Limité (Wayland) |
| Audio capture (cpal) | ✅ | ✅ | ✅ |
| Whisper.cpp (whisper-rs) | ✅ Metal | ✅ CPU | ✅ CPU |
| Accélération GPU | ✅ Metal | ⚠️ CUDA opt. | ⚠️ CUDA/Vulkan opt. |
| Configuration bundle Tauri | ✅ | ✅ | ✅ |
| Frontend adapté | ✅ | ✅ | ✅ |
| Script deps Linux | - | - | ✅ `install-deps.sh` |
| Doc Wayland | - | - | ✅ `WAYLAND_LIMITATIONS.md` |
| Compilation Rust | ✅ | Non testé | ✅ **Fonctionne** |

### 🟡 Reste à faire

| Composant | Description | Priorité |
|-----------|-------------|----------|
| Tests Windows | Compilation et exécution non testées localement | 🟡 Moyenne |

### ✅ CI/CD existante

| Workflow | Description |
|----------|-------------|
| `.github/workflows/ci.yml` | Check + Lint sur 4 plateformes (macOS ARM/x64, Windows, Linux) |
| `.github/workflows/release.yml` | Build + Release automatisée (DMG, NSIS, AppImage, DEB) |

---

## Phase 1: Scripts d'installation des dépendances

### 1.1 Linux (Debian/Ubuntu)

**Fichier:** `install-deps.sh` ✅ **Existe déjà**

**Pour GPU (optionnel):**
```bash
# NVIDIA CUDA
sudo apt install nvidia-cuda-toolkit

# Vulkan
sudo apt install vulkan-tools libvulkan-dev
```

### 1.2 Windows

**Fichier:** `install-deps.ps1` ou documentation

- Visual Studio Build Tools
- WebView2 (généralement préinstallé)
- Rust toolchain

### 1.3 macOS

Déjà fonctionnel avec Xcode Command Line Tools.

---

## Phase 2: Téléchargement des modèles Whisper

### 2.1 Script de téléchargement

**Fichier:** `scripts/download-models.sh`

```bash
#!/bin/bash
MODELS_DIR="src-tauri/models"
mkdir -p "$MODELS_DIR"

# Modèle small (recommandé pour démarrer)
# Note: On télécharge la version quantifiée mais on la renomme sans le suffixe
curl -L -o "$MODELS_DIR/ggml-small.bin" \
  "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small-q5_1.bin"

# Modèle large-v3-turbo (meilleure qualité)
curl -L -o "$MODELS_DIR/ggml-large-v3-turbo.bin" \
  "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin"
```

### 2.2 Téléchargement in-app (optionnel)

Ajouter une commande Tauri pour télécharger les modèles depuis l'interface.

---

## Phase 3: Tests de compilation multiplateforme

### 3.1 Compilation Linux (actuel)

```bash
cd src-tauri
cargo check                    # Vérifie la compilation
cargo build --release          # Build optimisé
```

### 3.2 Cross-compilation (optionnel)

Pour tester sans machine native:

```bash
# Windows depuis Linux
rustup target add x86_64-pc-windows-msvc
cargo build --target x86_64-pc-windows-msvc

# Note: Nécessite des linkers spécifiques
```

### 3.3 CI/CD GitHub Actions

✅ **Déjà configurée** dans `.github/workflows/`:
- `ci.yml` : Vérification sur toutes les plateformes à chaque push/PR
- `release.yml` : Build et publication automatique lors d'un tag `v*`

---

## Phase 4: Corrections et améliorations

### 4.1 Fenêtre overlay Linux

✅ **Déjà documenté** dans `WAYLAND_LIMITATIONS.md`:
- Limitations Wayland détaillées
- Workarounds par compositor (Sway, GNOME, KDE)
- Recommandation d'utiliser X11

### 4.2 Accélération GPU

| Plateforme | Backend | Flag de compilation |
|------------|---------|---------------------|
| macOS | Metal | Automatique |
| Windows/Linux | CUDA | `--features gpu-cuda` |
| Linux | ROCm | `--features gpu-hipblas` |
| Tous | Vulkan | `--features gpu-vulkan` |

### 4.3 Icônes

Vérifier que toutes les icônes sont présentes:
- `icon.icns` (macOS)
- `icon.ico` (Windows)
- `*.png` (Linux)

---

## Ordre d'exécution recommandé

### Étape 1: Tester la compilation actuelle
```bash
cd /home/clement/S2Tui/src-tauri
cargo build --release
```

### Étape 2: Installer les dépendances Linux (si nécessaire)
```bash
./install-deps.sh
```

### Étape 3: Télécharger les modèles Whisper
```bash
./scripts/download-models.sh
```

### Étape 4: Tester l'application complète
```bash
npm run tauri dev
```

---

## Résumé des fichiers à créer/modifier

### ✅ Corrigé

| Action | Fichier | Description |
|--------|---------|-------------|
| ✅ | `.github/workflows/ci.yml` | Nommage simplifié (`ggml-{model}.bin`) |
| ✅ | `.github/workflows/release.yml` | Nommage simplifié (`ggml-{model}.bin`) |

### ✅ Déjà fait

| Fichier | Status |
|---------|--------|
| `install-deps.sh` | ✅ Complet |
| `WAYLAND_LIMITATIONS.md` | ✅ Complet |
| `src-tauri/src/platform/` | ✅ Complet pour les 3 OS |
| `.github/workflows/ci.yml` | ✅ CI multiplateforme |
| `.github/workflows/release.yml` | ✅ Release automatisée |

---

## Notes techniques

### Dépendances Linux requises

```
libasound2-dev      # Audio (ALSA)
libssl-dev          # SSL/TLS
libgtk-3-dev        # GTK pour Tauri
libwebkit2gtk-4.1-dev  # WebView
```

### Limitations connues

1. **Wayland**: L'overlay "always-on-top" et "click-through" ne fonctionnent pas comme sur X11
2. **GPU Linux**: CUDA nécessite les drivers NVIDIA propriétaires
3. **Permissions Linux**: Pas de dialogue système, l'utilisateur doit être dans le groupe `audio`

### Fonctionnalité retirée

- **Auto-insert**: La fonctionnalité d'insertion automatique de texte a été retirée. L'application utilise uniquement le presse-papiers via `tauri-plugin-clipboard-manager`.

---

## Optimisation des performances (Linux/Windows)

### Pourquoi c'est plus lent que macOS ?

macOS utilise **Metal GPU** automatiquement, tandis que Linux/Windows utilisent le **CPU seul** par défaut.

### Configuration actuelle

| Paramètre | Description |
|-----------|-------------|
| `opt-level = "s"` | Compilation optimisée pour la taille du binaire |
| `n_threads = 75%` | Utilise 75% des cores CPU (laisse de la marge pour l'UI) |

### Dépendances optionnelles pour améliorer les performances

```bash
# OpenBLAS - accélère les opérations matricielles sur CPU
sudo apt install libopenblas-dev

# Vulkan - pour activer l'accélération GPU (AMD/Intel/NVIDIA)
sudo apt install libvulkan-dev vulkan-tools
```

### Activer l'accélération GPU

| GPU | Feature | Commande |
|-----|---------|----------|
| AMD (Radeon) | Vulkan | `cargo build --release --features gpu-vulkan` |
| NVIDIA | CUDA | `cargo build --release --features gpu-cuda` |
| AMD ROCm | HIPBlas | `cargo build --release --features gpu-hipblas` |

**Note**: L'accélération GPU nécessite les SDK appropriés (Vulkan SDK, CUDA Toolkit, ROCm).
