# Wayland Limitations on Linux

S2Tui works best on X11. While it runs on Wayland, some features have limitations due to Wayland's security model.

## Window Management Limitations

### What Works
- ✅ Basic window display
- ✅ Audio capture
- ✅ Speech recognition
- ✅ Clipboard operations

### What Has Limited Support
- ⚠️ **Always-on-top behavior**: May not work depending on the compositor
- ⚠️ **Skip taskbar**: Not universally supported across compositors
- ⚠️ **No-focus windows**: Limited support on Wayland
- ⚠️ **Multi-workspace visibility**: Compositor-dependent

### Why These Limitations Exist

Wayland enforces stricter security boundaries than X11. Window hints like `_NET_WM_STATE_*` are X11-specific and don't have direct Wayland equivalents. Each Wayland compositor (GNOME Shell, KDE Plasma, Sway, etc.) implements window management differently.

## Workarounds

### 1. Use X11 Session (Recommended)
Most Linux distributions offer both X11 and Wayland sessions. To use X11:

**On GNOME:**
- Log out
- Click the gear icon on the login screen
- Select "GNOME on Xorg"

**On KDE Plasma:**
- Log out
- Select "Plasma (X11)" from the session dropdown

### 2. Compositor-Specific Configurations

**Sway (i3-compatible):**
```
# In ~/.config/sway/config
for_window [app_id="s2tui"] floating enable, sticky enable
```

**GNOME Shell:**
Use extensions like "Always On Top" or "Window Calls Extended"

**KDE Plasma:**
Window Rules can be configured in System Settings > Window Management

## Detection

S2Tui automatically detects if you're running on Wayland and logs warnings:
```
Linux: Running on Wayland - overlay configuration has limited support
       For full overlay features, use X11 session
```

## Environment Variables Checked

S2Tui checks these environment variables to detect Wayland:
- `WAYLAND_DISPLAY`
- `XDG_SESSION_TYPE`

## Compositor Support Matrix

| Compositor | Always-on-top | Skip Taskbar | No-focus | Notes |
|------------|---------------|--------------|----------|-------|
| GNOME Shell | ⚠️ Limited | ❌ No | ⚠️ Limited | Extensions may help |
| KDE Plasma | ✅ Yes | ✅ Yes | ✅ Yes | Best Wayland support |
| Sway | ✅ Yes | ✅ Yes | ✅ Yes | Via window rules |
| Hyprland | ✅ Yes | ✅ Yes | ✅ Yes | Via window rules |
| wlroots-based | ⚠️ Varies | ⚠️ Varies | ⚠️ Varies | Depends on compositor |

## Recommended Setup

For the best S2Tui experience on Linux:
1. **Use X11 session** if possible
2. If using Wayland, prefer **KDE Plasma** or **Sway**
3. Configure compositor-specific window rules as needed

## Reporting Issues

If you encounter Wayland-specific issues:
1. Verify you're on Wayland: `echo $XDG_SESSION_TYPE`
2. Check compositor version
3. Include compositor logs when reporting issues
