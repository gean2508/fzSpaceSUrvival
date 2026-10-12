#ifndef IAFZ_64_H
#define IAFZ_64_H

/* ==============================================================================
 * KERNEL: IAFZ-64 — Red Neuronal Híbrida Ligera (Binaria + Proporcional)
 * VERSIÓN: AEGIS-HIBRIDO V9.0.0 (Optimizada para 64 bits y Caché L3)
 * ARCHIVO: kernel/iafz_64.h
 *
 * ARQUITECTURA:
 * - Capa 1 (Hidden): 64 Neuronas. Entrada = 32 bits (Sensores) + 32 bits (Memoria).
 * - Capa 2 (Strategy): 64 Neuronas.
 * - Salida Acción: 64 Pesos flotantes.
 * - Salida Memoria: 32 Estados flotantes recurrentes.
 *
 * TAMAÑOS CRÍTICOS:
 * Neuron64 = 9 bytes exactos (con #pragma pack(1) para contención térmica)
 * Brain64  = 9600 bytes exactos
 * ============================================================================== */

#include <stdint.h>
#include <stdio.h>

#define IAFZ_VERSION        "64-AEGIS-HIBRIDO-V9.0.0"
#define IAFZ_BRAIN_BYTES    11392  /* neuronas de 128 bits + 4 salidas */    

#define MAX_AGENTS          128   /* dron: poblacion del gym, no un enjambre */
#define NEURONS             64
#define MEMORY_BITS         32
/* La entrada pasa de 32 a 96 bits de sensores. Con 32 no entra nada visual sin
 * sacrificar el estado de vuelo: ocho magnitudes daban 4 bits cada una. El
 * contexto queda en 128 bits = 96 de sensores + 32 de memoria, y cada neurona
 * pasa a pesar dos palabras de 64. Cuesta dos popcount en vez de uno, sobre una
 * capa binaria que era el 26% del tiempo. */
#define SENSOR_BITS         96
/* Un dron real necesita CUATRO consignas (alabeo, cabeceo, guinada, altura) y
 * el kernel original daba una sola. Se agregan vectores de pesos de salida, NO
 * cerebros nuevos: las dos capas binarias -lo caro- se calculan una vez y se
 * comparten. Cada salida extra cuesta 256 bytes, no 9600. */
#define SALIDAS             4

#pragma pack(push, 1)

typedef struct {
    uint64_t weights[2]; // 16 bytes: 128 bits de contexto
    uint8_t  threshold;  // 1 byte
} Neuron64;              // 17 bytes totales

typedef struct {
    Neuron64 hidden[NEURONS];                    // 64 * 17 = 1088 bytes
    Neuron64 strategy[NEURONS];                  // 64 * 17 = 1088 bytes
    float    out_action_weights[SALIDAS][NEURONS]; // 4 * 64 * 4 = 1024 bytes
    float    memory_weights[MEMORY_BITS][NEURONS]; // 32 * 64 * 4 = 8192 bytes
} Brain64;                                       // Total = 11392 bytes

#pragma pack(pop)

_Static_assert(sizeof(Neuron64) == 17, "Falla crítica: Alineación de Neuron64.");
_Static_assert(sizeof(Brain64) == IAFZ_BRAIN_BYTES, "Falla crítica: Tamaño de Brain64 inconsistente.");

/* Operación central: XNOR + popcount sobre 128 bits (dos palabras). */
#define THINK_128(lo, hi, dna)                                          \
    ((__builtin_popcountll(~((lo) ^ (dna).weights[0])) +                \
      __builtin_popcountll(~((hi) ^ (dna).weights[1]))) >= (dna).threshold)

void  init_iafz_64(int id, uint32_t seed);
/* Sensores: 96 bits repartidos en `lo` (0..63) y `hi` (64..95, en los 32 bajos).
 * Los 32 de memoria recurrente los agrega el kernel. */
float think_iafz_64(int id, uint64_t sens_lo, uint32_t sens_hi);
/* Igual pero devuelve las SALIDAS consignas de una sola pasada. */
void  think_multi_iafz_64(int id, uint64_t sens_lo, uint32_t sens_hi, float *out);
void  mutate_iafz_64(int source_id, int target_id, float rate);
void  crossover_iafz_64(int p1, int p2, int child);
/* Empujon de grano FINO para la adaptacion en vuelo: copia el genoma y le
 * voltea `bits` bits sueltos (no medias palabras, como hace mutate) y corre
 * los pesos de salida un `paso` chico. mutate_iafz_64 hace `w ^= xorshift64()`,
 * que invierte ~32 de 64 bits de una: sirve para explorar desde cero en el
 * gym, pero en vuelo eso no es refinar, es saltar a otro genoma cualquiera. */
void  nudge_iafz_64(int source_id, int target_id, int bits, float paso, uint32_t seed);
void  reset_echo_iafz_64(int id);
/* Activaciones de la última decisión (capa oculta, capa de estrategia, memoria
 * recurrente actual), para mostrar en el visor. No es un cómputo nuevo: sólo
 * expone lo que think_*_iafz_64 ya calculó internamente. */
void  leer_activaciones_iafz_64(int id, uint64_t *l1, uint64_t *l2, uint32_t *mem);
void  earthquake_iafz_64(int id_target, int flip_percentage);
/* Volcar / restaurar un genoma en MEMORIA (IAFZ_BRAIN_BYTES bytes).
 * Es lo que necesita el repertorio de comportamientos: en el aire no se
 * guardan archivos, se copia un bloque de flash o de RAM. Mismo formato
 * exacto que save/load, asi que un genoma volcado se puede escribir a disco
 * tal cual y al reves. */
void  export_iafz_64(int id, unsigned char *buf);
void  import_iafz_64(int id, const unsigned char *buf);
int   save_iafz_64(int id, const char *filename);
int   load_iafz_64(int id, const char *filename);
int   load_bytes_iafz_64(int id, const unsigned char *data, int len);

#endif