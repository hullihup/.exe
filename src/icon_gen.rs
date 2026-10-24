// Wird von main.rs (Fenster-Icon) und build.rs (.exe-Icon) eingebunden.

/// Erzeugt das Programm-Icon als RGBA (Kantenlänge = Vielfaches von 16).
pub fn icon_rgba(size: usize) -> Vec<u8> {
    const G: [&str; 16] = [
        "................",
        "..############..",
        ".##############.",
        ".##############.",
        ".##############.",
        ".##############.",
        ".##WW######WW##.",
        ".##WP######PW##.",
        ".##############.",
        ".######SS######.",
        ".#####MMMM#####.",
        ".##############.",
        ".##############.",
        ".##############.",
        "..############..",
        "................",
    ];
    let shades: [[u8; 3]; 4] = [[27, 63, 115], [30, 76, 136], [34, 88, 155], [23, 55, 95]];
    let s = size / 16;
    let mut out = vec![0u8; size * size * 4];
    for gy in 0..16 {
        for gx in 0..16 {
            let ch = G[gy].as_bytes()[gx];
            let h = ((gx as u32).wrapping_mul(2654435761) ^ (gy as u32).wrapping_mul(40503)).rotate_left(7);
            let c: [u8; 4] = match ch {
                b'#' => {
                    let sh = shades[(h % 4) as usize];
                    [sh[0], sh[1], sh[2], 255]
                }
                b'W' => [245, 245, 250, 255],
                b'P' => [70, 200, 255, 255],
                b'S' => [20, 44, 78, 255],
                b'M' => [120, 200, 255, 255],
                _ => [0, 0, 0, 0],
            };
            for y in 0..s {
                for x in 0..s {
                    let o = ((gy * s + y) * size + gx * s + x) * 4;
                    out[o..o + 4].copy_from_slice(&c);
                }
            }
        }
    }
    out
}
