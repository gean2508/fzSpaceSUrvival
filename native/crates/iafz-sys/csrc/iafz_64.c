/* ==============================================================================
 * PROYECTO:    HIVE_OS / IAFZ-64
 * MÓDULO:      kernel/iafz_64.c
 * VERSIÓN:     9.0.0 — Kernel Ligero Híbrido
 * ============================================================================== */

#include "iafz_64.h"
#include <stdlib.h>
#include <string.h>

/* Memoria estática del enjambre */
static Brain64   swarm[MAX_AGENTS];
static uint32_t  recurrent_32[MAX_AGENTS]; // La memoria ahora ocupa 32 bits
static uint64_t  rng_state[MAX_AGENTS];

/* XORShift64: Determinista por ID, con vacuna anti-colapso a 0 */
static uint64_t xorshift64(int id) {
    uint64_t x = rng_state[id];
    if (x == 0ULL) x = 0x9E3779B97F4A7C15ULL; 
    x ^= x << 13; 
    x ^= x >> 7; 
    x ^= x << 17;
    return rng_state[id] = x;
}

/* Generador de floats en [-1.0, 1.0] sin sesgo de signo */
static float random_float(int id) {
    return ((float)(xorshift64(id) % 2000001) / 1000000.0f) - 1.0f;
}

void init_iafz_64(int id, uint32_t seed) {
    if (id < 0 || id >= MAX_AGENTS) return;
    
    rng_state[id] = (uint64_t)seed + (uint64_t)id + 0x9E3779B97F4A7C15ULL;
    if (rng_state[id] == 0ULL) rng_state[id] = 1ULL;
    
    for (int i = 0; i < NEURONS; i++) {
        swarm[id].hidden[i].weights   = xorshift64(id);
        // Umbrales biológicos ajustados para 64 bits (Rango 20 a 44)
        swarm[id].hidden[i].threshold = 20 + (uint8_t)(xorshift64(id) % 25);
        
        swarm[id].strategy[i].weights   = xorshift64(id);
        swarm[id].strategy[i].threshold = 20 + (uint8_t)(xorshift64(id) % 25);
        
        swarm[id].out_action_weights[i] = random_float(id);
        for(int m = 0; m < MEMORY_BITS; m++) {
            swarm[id].memory_weights[m][i] = random_float(id);
        }
    }
    recurrent_32[id] = 0;
}

float think_iafz_64(int id, uint32_t exteroception_32) {
    if (id < 0 || id >= MAX_AGENTS) return 0.0f;
    
    // FUSIÓN DE CONTEXTO: 32 bits de sensores (High) + 32 bits de memoria (Low)
    uint64_t context_64 = ((uint64_t)exteroception_32 << 32) | (uint64_t)recurrent_32[id];
    
    uint64_t l1_out = 0;
    uint64_t l2_out = 0;

    // CAPA 1: HIDDEN
    for (int i = 0; i < NEURONS; i++) {
        if (THINK_64(context_64, swarm[id].hidden[i])) {
            l1_out |= (1ULL << i);
        }
    }

    // CAPA 2: STRATEGY
    for (int i = 0; i < NEURONS; i++) {
        if (THINK_64(l1_out, swarm[id].strategy[i])) {
            l2_out |= (1ULL << i);
        }
    }

    // CAPA 3: OUTPUT HÍBRIDO (Acción + Memoria)
    float action = 0.0f;
    uint32_t next_mem = 0;
    
    for (int i = 0; i < NEURONS; i++) {
        // Acumular acción analógica
        if ((l2_out >> i) & 1) {
            action += swarm[id].out_action_weights[i];
        }
    }

    // Calcular los 32 estados de memoria flotante
    for (int m = 0; m < MEMORY_BITS; m++) {
        float m_acc = 0.0f;
        for (int i = 0; i < NEURONS; i++) {
            if ((l2_out >> i) & 1) {
                m_acc += swarm[id].memory_weights[m][i];
            }
        }
        // Si el acumulado es positivo, el bit de memoria se enciende para el siguiente frame
        if (m_acc > 0.0f) {
            next_mem |= (1U << m);
        }
    }
    
    recurrent_32[id] = next_mem;
    return action;
}

void mutate_iafz_64(int sid, int tid, float rate) {
    /* FIX: sin esto, un id fuera de rango escribe fuera del arreglo. */
    if (sid < 0 || sid >= MAX_AGENTS || tid < 0 || tid >= MAX_AGENTS) return;
    if (sid != tid) {
        memcpy(&swarm[tid], &swarm[sid], sizeof(Brain64));
        rng_state[tid] = xorshift64(sid); 
    }

    /* FIX: 4294967295.0f redondea a 2^32 y el cast a uint32_t es UB. */
    uint32_t prob = (rate >= 1.0f) ? 0xFFFFFFFFu
                  : (rate <= 0.0f) ? 0u
                  : (uint32_t)((double)rate * 4294967296.0);
    for (int i = 0; i < NEURONS; i++) {
        // Capa 1
        if ((uint32_t)xorshift64(tid) < prob) swarm[tid].hidden[i].weights ^= xorshift64(tid);
        if ((uint32_t)xorshift64(tid) < (prob / 2)) 
            swarm[tid].hidden[i].threshold = 20 + (uint8_t)(xorshift64(tid) % 25);

        // Capa 2
        if ((uint32_t)xorshift64(tid) < prob) swarm[tid].strategy[i].weights ^= xorshift64(tid);
        if ((uint32_t)xorshift64(tid) < (prob / 2)) 
            swarm[tid].strategy[i].threshold = 20 + (uint8_t)(xorshift64(tid) % 25);
        
        // Acción Analógica
        if ((uint32_t)xorshift64(tid) < prob) 
            swarm[tid].out_action_weights[i] += random_float(tid) * 0.05f;

        // Memoria Recurrente
        for (int m = 0; m < MEMORY_BITS; m++) {
            if ((uint32_t)xorshift64(tid) < prob) {
                swarm[tid].memory_weights[m][i] += random_float(tid) * 0.05f;
            }
        }
    }
}

void crossover_iafz_64(int p1, int p2, int child) {
    if (p1 < 0 || p2 < 0 || child < 0 || p1 >= MAX_AGENTS || p2 >= MAX_AGENTS || child >= MAX_AGENTS) return;
    
    rng_state[child] = xorshift64(p1) ^ xorshift64(p2);
    if (rng_state[child] == 0) rng_state[child] = 1;

    for (int i = 0; i < NEURONS; i++) {
        if (xorshift64(child) % 2 == 0) swarm[child].hidden[i] = swarm[p1].hidden[i];
        else                            swarm[child].hidden[i] = swarm[p2].hidden[i];

        if (xorshift64(child) % 2 == 0) swarm[child].strategy[i] = swarm[p1].strategy[i];
        else                            swarm[child].strategy[i] = swarm[p2].strategy[i];

        if (xorshift64(child) % 2 == 0) swarm[child].out_action_weights[i] = swarm[p1].out_action_weights[i];
        else                            swarm[child].out_action_weights[i] = swarm[p2].out_action_weights[i];

        for (int m = 0; m < MEMORY_BITS; m++) {
            if (xorshift64(child) % 2 == 0) swarm[child].memory_weights[m][i] = swarm[p1].memory_weights[m][i];
            else                            swarm[child].memory_weights[m][i] = swarm[p2].memory_weights[m][i];
        }
    }
    recurrent_32[child] = 0;
}

void earthquake_iafz_64(int id, int p) {
    if (id < 0 || id >= MAX_AGENTS) return;   /* FIX: mismo agujero que mutate */
    uint32_t prob = (p >= 100) ? 0xFFFFFFFFu
                  : (p <= 0)   ? 0u
                  : (uint32_t)(((double)p/100.0) * 4294967296.0);
    for(int i = 0; i < NEURONS; i++) {
        if((uint32_t)xorshift64(id) < prob) {
            swarm[id].hidden[i].weights   = xorshift64(id);
            swarm[id].hidden[i].threshold = 20 + (uint8_t)(xorshift64(id) % 25);
            
            swarm[id].strategy[i].weights   = xorshift64(id);
            swarm[id].strategy[i].threshold = 20 + (uint8_t)(xorshift64(id) % 25);
            
            swarm[id].out_action_weights[i] = random_float(id);
            
            for(int m = 0; m < MEMORY_BITS; m++) {
                swarm[id].memory_weights[m][i] = random_float(id);
            }
        }
    }
    recurrent_32[id] = 0; 
}

int save_iafz_64(int id, const char *filename) {
    FILE *fp = fopen(filename, "wb");
    if(!fp) return 0;
    size_t written = fwrite(&swarm[id], sizeof(Brain64), 1, fp);
    fclose(fp);
    return (written == 1);
}

int load_iafz_64(int id, const char *filename) {
    FILE *fp = fopen(filename, "rb");
    if(!fp) return 0;
    fseek(fp, 0, SEEK_END);
    if(ftell(fp) != sizeof(Brain64)) { fclose(fp); return 0; }
    rewind(fp);
    size_t readed = fread(&swarm[id], sizeof(Brain64), 1, fp);
    fclose(fp);
    return (readed == 1);
}

void reset_echo_iafz_64(int id) {
    if (id >= 0 && id < MAX_AGENTS) recurrent_32[id] = 0;
}
/* Añadido (vendor): cargar un genoma desde MEMORIA (para embeberlo en el juego,
   sin archivo — assets del APK). Copia crudo en swarm[id] si el tamaño calza. */
#include <string.h>
int load_bytes_iafz_64(int id, const unsigned char *data, int len) {
    if (id < 0 || id >= MAX_AGENTS) return 0;
    if (data == 0 || len != (int)sizeof(Brain64)) return 0;
    memcpy(&swarm[id], data, sizeof(Brain64));
    recurrent_32[id] = 0;
    return 1;
}
