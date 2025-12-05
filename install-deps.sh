#!/bin/bash
# Installation des dépendances nécessaires pour compiler S2Tui sur Linux

echo "Installation des dépendances pour S2Tui..."
echo "Mise à jour des paquets..."
sudo apt-get update

echo "Installation des outils de build et dépendances Tauri..."
sudo apt-get install -y \
    build-essential \
    cmake \
    clang \
    libclang-dev \
    pkg-config \
    libssl-dev \
    libgtk-3-dev \
    libwebkit2gtk-4.1-dev \
    libayatana-appindicator3-dev \
    librsvg2-dev \
    patchelf \
    libasound2-dev

echo ""
echo "✅ Installation terminée !"
echo ""
echo "Vous pouvez maintenant compiler le projet avec:"
echo "  cd src-tauri && cargo check"
echo "ou lancer l'app en mode dev:"
echo "  npm run tauri dev"
