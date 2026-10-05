//! SHA-256 (FIPS 180-4), lo justo para el compromiso de la muestra sellada.
//!
//! Va escrito a mano para que el crate no tenga dependencias. Los tests lo
//! comparan con los vectores de la norma y con una implementación externa en
//! los largos donde el relleno cambia de forma.

/// Primeros 32 bits de la parte fraccionaria de las raíces cúbicas de los
/// primeros 64 primos.
const K: [u32; 64] = [
    0x428a_2f98,
    0x7137_4491,
    0xb5c0_fbcf,
    0xe9b5_dba5,
    0x3956_c25b,
    0x59f1_11f1,
    0x923f_82a4,
    0xab1c_5ed5,
    0xd807_aa98,
    0x1283_5b01,
    0x2431_85be,
    0x550c_7dc3,
    0x72be_5d74,
    0x80de_b1fe,
    0x9bdc_06a7,
    0xc19b_f174,
    0xe49b_69c1,
    0xefbe_4786,
    0x0fc1_9dc6,
    0x240c_a1cc,
    0x2de9_2c6f,
    0x4a74_84aa,
    0x5cb0_a9dc,
    0x76f9_88da,
    0x983e_5152,
    0xa831_c66d,
    0xb003_27c8,
    0xbf59_7fc7,
    0xc6e0_0bf3,
    0xd5a7_9147,
    0x06ca_6351,
    0x1429_2967,
    0x27b7_0a85,
    0x2e1b_2138,
    0x4d2c_6dfc,
    0x5338_0d13,
    0x650a_7354,
    0x766a_0abb,
    0x81c2_c92e,
    0x9272_2c85,
    0xa2bf_e8a1,
    0xa81a_664b,
    0xc24b_8b70,
    0xc76c_51a3,
    0xd192_e819,
    0xd699_0624,
    0xf40e_3585,
    0x106a_a070,
    0x19a4_c116,
    0x1e37_6c08,
    0x2748_774c,
    0x34b0_bcb5,
    0x391c_0cb3,
    0x4ed8_aa4a,
    0x5b9c_ca4f,
    0x682e_6ff3,
    0x748f_82ee,
    0x78a5_636f,
    0x84c8_7814,
    0x8cc7_0208,
    0x90be_fffa,
    0xa450_6ceb,
    0xbef9_a3f7,
    0xc671_78f2,
];

/// Primeros 32 bits de la parte fraccionaria de las raíces cuadradas de los
/// primeros 8 primos.
const INICIAL: [u32; 8] = [
    0x6a09_e667,
    0xbb67_ae85,
    0x3c6e_f372,
    0xa54f_f53a,
    0x510e_527f,
    0x9b05_688c,
    0x1f83_d9ab,
    0x5be0_cd19,
];

/// Resumen SHA-256 de `datos`.
pub(crate) fn sha256(datos: &[u8]) -> [u8; 32] {
    let mut estado = INICIAL;
    let mut bloques = datos.chunks_exact(64);
    for bloque in &mut bloques {
        comprimir(&mut estado, bloque);
    }

    // Relleno: un bit 1, ceros, y el largo en bits como u64 big-endian, hasta
    // completar un múltiplo de 64 bytes. Si al resto no le caben el 0x80 y los
    // 8 bytes del largo, hace falta un bloque más.
    let resto = bloques.remainder();
    let mut cola = [0u8; 128];
    cola[..resto.len()].copy_from_slice(resto);
    cola[resto.len()] = 0x80;
    let largo_cola = if resto.len() < 56 { 64 } else { 128 };
    let bits = (datos.len() as u64).wrapping_mul(8);
    cola[largo_cola - 8..largo_cola].copy_from_slice(&bits.to_be_bytes());
    for bloque in cola[..largo_cola].chunks_exact(64) {
        comprimir(&mut estado, bloque);
    }

    let mut resumen = [0u8; 32];
    for (destino, palabra) in resumen.chunks_exact_mut(4).zip(estado) {
        destino.copy_from_slice(&palabra.to_be_bytes());
    }
    resumen
}

/// Procesa un bloque de 64 bytes.
fn comprimir(estado: &mut [u32; 8], bloque: &[u8]) {
    let mut w = [0u32; 64];
    for (t, palabra) in bloque.chunks_exact(4).enumerate() {
        w[t] = u32::from_be_bytes([palabra[0], palabra[1], palabra[2], palabra[3]]);
    }
    for t in 16..64 {
        let s0 = w[t - 15].rotate_right(7) ^ w[t - 15].rotate_right(18) ^ (w[t - 15] >> 3);
        let s1 = w[t - 2].rotate_right(17) ^ w[t - 2].rotate_right(19) ^ (w[t - 2] >> 10);
        w[t] = w[t - 16]
            .wrapping_add(s0)
            .wrapping_add(w[t - 7])
            .wrapping_add(s1);
    }

    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *estado;
    for t in 0..64 {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let eleccion = (e & f) ^ (!e & g);
        let t1 = h
            .wrapping_add(s1)
            .wrapping_add(eleccion)
            .wrapping_add(K[t])
            .wrapping_add(w[t]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let mayoria = (a & b) ^ (a & c) ^ (b & c);
        let t2 = s0.wrapping_add(mayoria);
        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(t1);
        d = c;
        c = b;
        b = a;
        a = t1.wrapping_add(t2);
    }

    for (acumulado, nuevo) in estado.iter_mut().zip([a, b, c, d, e, f, g, h]) {
        *acumulado = acumulado.wrapping_add(nuevo);
    }
}

#[cfg(test)]
mod tests {
    use super::sha256;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Los tres ejemplos de la norma, más el del millón de letras `a`.
    #[test]
    fn vectores_de_la_norma() {
        assert_eq!(
            hex(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hex(&sha256(
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
            )),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        assert_eq!(
            hex(&sha256(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex(&sha256(&vec![b'a'; 1_000_000])),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    /// Largos alrededor de los bordes del relleno (55/56/57 y 63/64/65 bytes,
    /// 119/120), contra resúmenes calculados con una implementación externa.
    #[test]
    fn largos_en_los_bordes_del_relleno() {
        let patron = |n: usize| {
            (0..n)
                .map(|i| ((i * 7 + 3) % 256) as u8)
                .collect::<Vec<u8>>()
        };
        let esperados = [
            (
                1,
                "084fed08b978af4d7d196a7446a86b58009e636b611db16211b65a9aadff29c5",
            ),
            (
                55,
                "e7313d333c272e639f790978283f9eb392e843d0f29b7016828bb1daa4aac70b",
            ),
            (
                56,
                "4324d65f3c103567f5589c710bc08f8523f929a9272e3af36fc968e52abc6c27",
            ),
            (
                57,
                "35df609437dcfea3279283ab79fd554e2bf78f8f7ae2de532d8ee300b09e8f73",
            ),
            (
                63,
                "81c80242132f230c3bd41b3e63bbcff16107339549214a99614ff26664625055",
            ),
            (
                64,
                "39e3d7b6b5d075d37d053ad89b24b41bef4f3c29760c84447cab3f3be1882241",
            ),
            (
                65,
                "aacca6ff74fdbb296d165a45cecfa04e5127bc008770fbbdd48006f2d2fae95e",
            ),
            (
                119,
                "9ce7368e4daf32341631b492e80359dc9f594b48453cd0dd5bf0b19279cc177e",
            ),
            (
                120,
                "7836b787757e95e58b3ca5aec90b1b004e8deba1e50e9675af9cabf1a13a04b5",
            ),
            (
                1000,
                "1e9bc38cbf860b9ec31918b065f9b52476c549a782e0e7990bed8ce3868d2371",
            ),
        ];
        for (largo, esperado) in esperados {
            assert_eq!(hex(&sha256(&patron(largo))), esperado, "largo {largo}");
        }
    }
}
