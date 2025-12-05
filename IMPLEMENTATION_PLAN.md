# Plan d'Action : Corrections des Événements de Clic Windows/Linux

**Date** : 2025-12-05
**Objectif** : Résoudre le problème où les boutons microphone et settings ne répondent pas aux clics dans les versions compilées sur Windows et Linux

---

## 📋 Résumé des Modifications

### 1. Supprimer WS_EX_NOACTIVATE (Windows uniquement)
**Fichier** : `src-tauri/src/platform/windows.rs`
**Ligne** : 114
**Raison** : Ce flag empêche la fenêtre de recevoir les événements de clic correctement en production, surtout en combinaison avec le system tray

### 2. Corriger l'URL de la fenêtre Settings
**Fichier** : `src/composables/useTauri.ts`
**Ligne** : 31
**Raison** : Le slash initial (`/settings.html`) ne fonctionne pas avec le protocole `tauri://` en production

---

## 🎯 Modifications Détaillées

### Modification 1 : windows.rs

**Fichier** : `src-tauri/src/platform/windows.rs`

**Ligne actuelle (114)** :
```rust
let new_ex_style = current_ex_style | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST;
```

**Nouvelle ligne** :
```rust
let new_ex_style = current_ex_style | WS_EX_TOOLWINDOW | WS_EX_TOPMOST;
```

**Modification du log (ligne 124)** :
```rust
// Ancien
tracing::info!("Windows: Extended styles configured (NOACTIVATE | TOOLWINDOW | TOPMOST)");

// Nouveau
tracing::info!("Windows: Extended styles configured (TOOLWINDOW | TOPMOST)");
```

**Impact** :
- ✅ Les événements de clic seront propagés au webview
- ✅ Compatible avec le system tray
- ⚠️ La fenêtre prendra le focus quand on clique dessus (comportement standard)

**Note** : La constante `WS_EX_NOACTIVATE` (ligne 99) peut rester définie pour référence future, ou être supprimée si on ne prévoit pas de la réutiliser.

---

### Modification 2 : useTauri.ts

**Fichier** : `src/composables/useTauri.ts`

**Ligne actuelle (31)** :
```typescript
url: "/settings.html",
```

**Nouvelle ligne** :
```typescript
url: "settings.html",
```

**Impact** :
- ✅ L'URL sera correctement résolue en production (`tauri://localhost/settings.html`)
- ✅ Compatible avec le build Vite (déjà configuré dans `vite.config.ts:20`)
- ✅ Fonctionne en dev ET en production

---

## 🔍 Modifications Optionnelles (Non incluses dans ce plan)

### Option A : Modifier focus: false → focus: true

**Fichier** : `src-tauri/tauri.conf.json`
**Ligne** : 31

**Pourquoi ne pas le faire maintenant** :
- La suppression de `WS_EX_NOACTIVATE` devrait suffire à résoudre le problème
- On teste d'abord une modification minimale
- Si le problème persiste, cette modification sera la prochaine étape

### Option B : Désactiver temporairement le System Tray

**Fichier** : `src-tauri/src/lib.rs`
**Ligne** : 48

**Pourquoi ne pas le faire maintenant** :
- Le system tray est une fonctionnalité importante
- La suppression de `WS_EX_NOACTIVATE` devrait résoudre le conflit
- À envisager seulement si le problème persiste

---

## 📝 Procédure de Test

### Étape 1 : Vérifier les modifications
```bash
# Vérifier que les fichiers sont bien modifiés
git diff src-tauri/src/platform/windows.rs
git diff src/composables/useTauri.ts
```

### Étape 2 : Tester en mode dev (sanity check)
```bash
npm run tauri dev
```
**Tests** :
- ✅ Le bouton micro fonctionne
- ✅ Le bouton settings ouvre la fenêtre de configuration
- ✅ Aucune régression

### Étape 3 : Build pour Windows
```bash
npm run tauri build
```

### Étape 4 : Tests sur Windows (build de production)
**Tests critiques** :
1. ✅ Clic sur bouton micro → Démarre l'enregistrement
2. ✅ Clic sur bouton settings → Ouvre la fenêtre settings
3. ✅ System tray → Menu fonctionne
4. ⚠️ Observer : La fenêtre prend-elle le focus au clic ? (comportement attendu)

### Étape 5 : Build pour Linux
```bash
npm run tauri build
```

### Étape 6 : Tests sur Linux (build de production)
**Tests critiques** :
1. ✅ Clic sur bouton micro → Démarre l'enregistrement
2. ✅ Clic sur bouton settings → Ouvre la fenêtre settings
3. ✅ Tester sur X11 ET Wayland si possible

---

## ⚠️ Impacts Attendus

### Comportement Changé (Normal)

**Avant** :
- Clic sur overlay → Fenêtre ne prend pas le focus
- Application en arrière-plan reste active
- ❌ Aucun événement de clic ne fonctionne (bug)

**Après** :
- Clic sur overlay → Fenêtre PREND le focus
- Application en arrière-plan perd le focus temporairement
- ✅ Tous les événements de clic fonctionnent

### Scénarios d'Utilisation

**Scénario 1 : Transcription dans Google Docs**
```
Avant (bugué) :
1. Utilisateur tape dans Google Docs
2. Clic sur micro → ❌ Rien ne se passe
3. 10 clics → ❌ Toujours rien

Après (corrigé) :
1. Utilisateur tape dans Google Docs
2. Clic sur micro → ✅ Enregistrement démarre
3. ⚠️ Google Docs perd le focus
4. Re-cliquer sur Google Docs pour continuer à taper
```

**Note** : C'est un compromis acceptable car :
- Sans ce fix, l'app ne fonctionne PAS DU TOUT
- Avec ce fix, l'app fonctionne mais nécessite un re-focus manuel
- C'est le comportement standard de la plupart des overlays Windows

---

## 🚀 Rollback Plan

Si les modifications causent des problèmes :

### Rollback Complet
```bash
git checkout src-tauri/src/platform/windows.rs
git checkout src/composables/useTauri.ts
npm run tauri build
```

### Rollback Partiel (garder seulement le fix settings URL)
```bash
# Rollback seulement windows.rs
git checkout src-tauri/src/platform/windows.rs

# Garder useTauri.ts modifié
npm run tauri build
```

---

## 📊 Checklist de Validation

### Avant de Commit
- [ ] Code modifié dans `windows.rs` ligne 114
- [ ] Code modifié dans `useTauri.ts` ligne 31
- [ ] Log mis à jour dans `windows.rs` ligne 124
- [ ] Compilation Rust réussie (`cd src-tauri && cargo check`)
- [ ] Compilation TypeScript réussie (`vue-tsc --noEmit`)

### Tests Dev
- [ ] Mode dev fonctionne normalement
- [ ] Bouton micro fonctionne
- [ ] Bouton settings fonctionne
- [ ] Aucune erreur console

### Tests Production Windows
- [ ] Build Windows réussi
- [ ] Bouton micro fonctionne
- [ ] Bouton settings ouvre la fenêtre
- [ ] System tray fonctionne
- [ ] Pas d'erreurs visibles

### Tests Production Linux
- [ ] Build Linux réussi
- [ ] Bouton micro fonctionne (X11)
- [ ] Bouton settings ouvre la fenêtre (X11)
- [ ] Test sur Wayland si disponible

---

## 🔮 Prochaines Étapes (si problème persiste)

### Si les clics ne fonctionnent toujours pas après ces modifications :

1. **Modifier focus: false → focus: true** dans `tauri.conf.json:31`
2. **Désactiver temporairement le system tray** pour isoler le problème
3. **Ajouter des logs détaillés** dans le frontend pour voir si les événements arrivent
4. **Ouvrir une issue GitHub** sur tauri-apps/tauri avec les détails spécifiques

### Si la prise de focus est trop intrusive :

1. **Implémenter WM_MOUSEACTIVATE** (solution avancée, nécessite plus de code natif)
2. **Évaluer si le shortcut global** peut remplacer le besoin de cliquer sur l'overlay
3. **Envisager une UI alternative** (ex: window minimale sans overlay transparent)

---

## 📚 Références

**Issues GitHub liées** :
- [#13389 - System tray causes unclickable window](https://github.com/tauri-apps/tauri/issues/13389)
- [#8869 - Window focus conflicts with SystemTrayEvent](https://github.com/tauri-apps/tauri/issues/8869)
- [wry#637 - First click not propagated](https://github.com/tauri-apps/wry/issues/637)

**Documentation** :
- [Microsoft - Extended Window Styles](https://learn.microsoft.com/en-us/windows/win32/winmsg/extended-window-styles)
- [Tauri v2 - Window Customization](https://v2.tauri.app/learn/window-customization/)

---

## ✅ Validation du Plan

**Ce plan est prêt pour implémentation si** :
- ✅ L'utilisateur accepte que la fenêtre prenne le focus au clic (compromis nécessaire)
- ✅ L'utilisateur veut d'abord tester avec modifications minimales
- ✅ L'utilisateur peut tester sur Windows ET Linux après build

**Actions requises de l'utilisateur** :
1. Approuver ce plan
2. Tester le build de production après modifications
3. Rapporter si le comportement est acceptable ou nécessite des ajustements supplémentaires
