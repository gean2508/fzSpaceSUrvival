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
#define IAFZ_BRAIN_BYTES    9600    

#define MAX_AGENTS          1000
#define NEURONS             64
#define MEMORY_BITS         32

#pragma pack(push, 1)

typedef struct {
    uint64_t weights;   // 8 bytes (Alineación natural x86_64)
    uint8_t  threshold; // 1 byte  (Umbral biológico)
} Neuron64;             // 9 bytes totales

typedef struct {
    Neuron64 hidden[NEURONS];                    // 64 * 9 = 576 bytes
    Neuron64 strategy[NEURONS];                  // 64 * 9 = 576 bytes
    float    out_action_weights[NEURONS];        // 64 * 4 = 256 bytes
    float    memory_weights[MEMORY_BITS][NEURONS]; // 32 * 64 * 4 = 8192 bytes
} Brain64;                                       // Total = 9600 bytes

#pragma pack(pop)

_Static_assert(sizeof(Neuron64) == 9, "Falla crítica: Alineación de Neuron64.");
_Static_assert(sizeof(Brain64) == IAFZ_BRAIN_BYTES, "Falla crítica: Tamaño de Brain64 inconsistente.");

/* Operación central ultrarrápida: 1 solo XOR y 1 solo Popcount por neurona */
#define THINK_64(input_64, dna)                                     \
    (__builtin_popcountll(~((input_64) ^ (dna).weights)) >= (dna).threshold)

void  init_iafz_64(int id, uint32_t seed);
float think_iafz_64(int id, uint32_t exteroception_32); 
void  mutate_iafz_64(int source_id, int target_id, float rate);
void  crossover_iafz_64(int p1, int p2, int child);
void  reset_echo_iafz_64(int id);
void  earthquake_iafz_64(int id_target, int flip_percentage);
int   save_iafz_64(int id, const char *filename);
int   load_iafz_64(int id, const char *filename);
int   load_bytes_iafz_64(int id, const unsigned char *data, int len);

#endif