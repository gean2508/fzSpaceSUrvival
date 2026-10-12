/* ==============================================================================
 * iafz_64_abordo.c — EL CEREBRO QUE VA EN EL CHIP.
 *
 * Sólo la inferencia de UN genoma: sin librería de C, sin malloc, sin
 * archivos, sin generador aleatorio. Es lo que se compila para el ESP32 (el
 * genoma, 11392 bytes, va en flash como `const`). Todo lo demás del kernel
 * (mutar, cruzar, guardar, la población de 128) es del entrenamiento en la
 * computadora y no sube nunca.
 *
 * **Tiene que dar EXACTAMENTE lo mismo que `think_multi_iafz_64`** —el cerebro
 * con el que se entrenó— bit a bit, o el genoma instalado sería otro dron
 * (regla 3.3). Lo verifica la prueba `abordo::igual_al_kernel_del_gym`, con
 * genomas y entradas al azar y la memoria recurrente evolucionando.
 *
 * Dos diferencias a propósito con `iafz_64.c`, y las dos dan el mismo número:
 *
 *  - popcount por SWAR (sumas y máscaras) en vez de `__builtin_popcountll`:
 *    ni el Xtensa del ESP32/ESP32-S3 ni el RISC-V del ESP32-C3 tienen
 *    instrucción de popcount, y el builtin compilaría a una llamada a
 *    biblioteca que en un chip pelado no existe. La regla 3.14 lo tenía
 *    anotado como «a verificar en el chip»: esto lo resuelve de antemano.
 *  - la estructura se declara acá (sin `stdio.h`), con los mismos tamaños
 *    comprobados en compilación.
 * ============================================================================== */
#include <stdint.h>

#define AB_NEURONAS  64
#define AB_MEMORIA   32
#define AB_SALIDAS   4

#pragma pack(push, 1)
typedef struct {
    uint64_t pesos[2];
    uint8_t  umbral;
} AbNeurona;

typedef struct {
    AbNeurona oculta[AB_NEURONAS];
    AbNeurona estrategia[AB_NEURONAS];
    float     salidas[AB_SALIDAS][AB_NEURONAS];
    float     memoria[AB_MEMORIA][AB_NEURONAS];
} AbCerebro;
#pragma pack(pop)

_Static_assert(sizeof(AbNeurona) == 17, "la neurona de a bordo no mide 17 bytes");
_Static_assert(sizeof(AbCerebro) == 11392, "el cerebro de a bordo no mide lo mismo que el del gym");

/* Lo que cambia de una decisión a la siguiente. */
typedef struct {
    uint64_t sensores_lo;   /* bits 0..63 de los sensores */
    uint32_t sensores_hi;   /* bits 64..95 */
    uint32_t memoria;       /* los 32 bits recurrentes: ENTRADA y SALIDA */
    float    salidas[AB_SALIDAS];
} AbEstado;

static inline __attribute__((always_inline)) uint32_t popcount64(uint64_t x) {
    x = x - ((x >> 1) & 0x5555555555555555ULL);
    x = (x & 0x3333333333333333ULL) + ((x >> 2) & 0x3333333333333333ULL);
    x = (x + (x >> 4)) & 0x0F0F0F0F0F0F0F0FULL;
    /* Sin multiplicación de 64 bits (costosa en un chip de 32): sumas. */
    x += x >> 8;
    x += x >> 16;
    x += x >> 32;
    return (uint32_t)(x & 0x7F);
}

static inline __attribute__((always_inline)) int dispara(uint64_t lo, uint64_t hi, const AbNeurona *n) {
    uint64_t w0, w1;
    /* Copia byte a byte: la estructura está empaquetada (17 bytes) y un
       acceso de 8 bytes desalineado revienta en un chip de 32 bits. */
    __builtin_memcpy(&w0, &n->pesos[0], 8);
    __builtin_memcpy(&w1, &n->pesos[1], 8);
    return (popcount64(~(lo ^ w0)) + popcount64(~(hi ^ w1))) >= n->umbral;
}

void pensar_abordo(const AbCerebro *c, AbEstado *e) {
    uint64_t ctx_lo = e->sensores_lo;
    uint64_t ctx_hi = ((uint64_t)e->memoria << 32) | (uint64_t)e->sensores_hi;

    uint64_t l1 = 0;
    for (int i = 0; i < AB_NEURONAS; i++) {
        if (dispara(ctx_lo, ctx_hi, &c->oculta[i])) l1 |= (1ULL << i);
    }
    uint64_t l2 = 0;
    for (int i = 0; i < AB_NEURONAS; i++) {
        if (dispara(l1, 0ULL, &c->estrategia[i])) l2 |= (1ULL << i);
    }

    /* Las neuronas de estrategia que dispararon, en orden. Las sumas de
       abajo recorren SÓLO esas (la mitad, en promedio) y en el mismo orden
       que el kernel del gym, así que el resultado es idéntico bit a bit y
       cuesta la mitad: la proyección de memoria era el 89% del trabajo
       flotante (regla 3.14) y dominaba la cuenta medida en el emulador. */
    uint8_t activas[AB_NEURONAS];
    int n = 0;
    for (int i = 0; i < AB_NEURONAS; i++) {
        if ((l2 >> i) & 1) activas[n++] = (uint8_t)i;
    }

    uint32_t memoria = 0;
    for (int m = 0; m < AB_MEMORIA; m++) {
        const float *w = c->memoria[m];
        float acc = 0.0f;
        for (int k = 0; k < n; k++) acc += w[activas[k]];
        if (acc > 0.0f) memoria |= (1U << m);
    }
    e->memoria = memoria;

    for (int o = 0; o < AB_SALIDAS; o++) {
        const float *w = c->salidas[o];
        float a = 0.0f;
        for (int k = 0; k < n; k++) a += w[activas[k]];
        e->salidas[o] = a;
    }
}
