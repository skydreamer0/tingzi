// Written by assets/design/build_tokens.py from assets/design/tokens.json; edit that file instead.
#pragma once
#include <windows.h>

namespace tingzi {
// The window's background, its text and quieter text, its edge, the chosen item's background and text.
struct Palette { COLORREF back, text, muted, frame, active, activeText; };
inline constexpr Palette lightPalette=Palette{RGB(255,255,255),RGB(31,36,85),RGB(84,90,128),RGB(190,196,228),RGB(228,229,241),RGB(48,56,151)};
inline constexpr Palette darkPalette=Palette{RGB(32,35,50),RGB(232,235,255),RGB(166,172,205),RGB(74,79,112),RGB(45,50,95),RGB(183,187,233)};
inline constexpr int chipRadius=6;
}
