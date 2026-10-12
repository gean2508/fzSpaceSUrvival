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
/* Patrón de disparo de la última decisión, por capa. Antes `forward_iafz_64`
   las calculaba y las tiraba —sólo salía la suma ponderada final—, así que
   no había forma de MOSTRAR qué neurona se prendió, sólo el resultado. El
   visor (panel "nuestra red") necesita esto para ser información real y no
   una animación inventada. */
static uint64_t  last_l1[MAX_AGENTS];
static uint64_t  last_l2[MAX_AGENTS];

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
        swarm[id].hidden[i].weights[0] = xorshift64(id);
        swarm[id].hidden[i].weights[1] = xorshift64(id);
        /* Umbral centrado en 128 bits: el punto medio es 64, no 32. */
        swarm[id].hidden[i].threshold = 44 + (uint8_t)(xorshift64(id) % 41);

        swarm[id].strategy[i].weights[0] = xorshift64(id);
        swarm[id].strategy[i].weights[1] = xorshift64(id);
        swarm[id].strategy[i].threshold = 44 + (uint8_t)(xorshift64(id) % 41);
        
        for (int o = 0; o < SALIDAS; o++) swarm[id].out_action_weights[o][i] = random_float(id);
        for(int m = 0; m < MEMORY_BITS; m++) {
            swarm[id].memory_weights[m][i] = random_float(id);
        }
    }
    recurrent_32[id] = 0;
}

/* Nucleo compartido: capas binarias + actualizacion de memoria. Devuelve el
 * patron de la capa de estrategia. Lo caro esta ACA, y se hace una sola vez
 * aunque despues se lean cuatro salidas. */
static uint64_t forward_iafz_64(int id, uint64_t sens_lo, uint32_t sens_hi) {
    /* 128 bits de contexto: 96 de sensores + 32 de memoria recurrente. */
    uint64_t ctx_lo = sens_lo;
    uint64_t ctx_hi = ((uint64_t)recurrent_32[id] << 32) | (uint64_t)sens_hi;

    uint64_t l1_out = 0;
    for (int i = 0; i < NEURONS; i++) {
        if (THINK_128(ctx_lo, ctx_hi, swarm[id].hidden[i])) l1_out |= (1ULL << i);
    }
    /* La segunda capa ve el patron de la primera en la palabra baja; la alta
       queda en cero, que es un contexto valido y constante.

       PROBADO PONERLE ctx_hi ACA (memoria recurrente + 32b de sensor) Y
       REVERTIDO POR MEDICION — bitacora 7.25. La idea era buena en papel y
       la bitacora 7.11 la tenia como "candidato fuerte y barato": esos 64
       bits altos comparados contra 0 constante valen
       popcount(~(0^w1)) = 64-popcount(w1), un numero fijo por neurona, o
       sea que 512 B de genoma por capa solo codifican un sesgo redundante
       con `threshold`. Poner ctx_hi ahi no cuesta un popcount ni un byte.

       Lo que paso al medirlo, misma semilla (1) que la corrida de control:

         kernel con [l1_out, 0ULL]   llega al peldaño 13 en gen 576,
                                     SIN desaprender ninguno de los 12
         kernel con [l1_out, ctx_hi] llega al peldaño 13 en gen  51,
                                     DESAPRENDIO los 12, sostenido 190
                                     generaciones (gen 104 a 240)

       O sea: aprende mas rapido y olvida mas. Mas capacidad no cierra el
       agujero del `min` del examen de repaso (regla 3.8) — le da a la
       busqueda MAS formas de reacomodarse pisando lo viejo, no menos.
       Es UNA semilla (seccion 4), asi que no es un veredicto cerrado: si
       alguien lo reintenta, que sea con varias semillas Y con algo que
       proteja lo aprendido, no solo con mas capacidad. */
    uint64_t l2_out = 0;
    for (int i = 0; i < NEURONS; i++) {
        if (THINK_128(l1_out, 0ULL, swarm[id].strategy[i])) l2_out |= (1ULL << i);
    }
    last_l1[id] = l1_out;
    last_l2[id] = l2_out;

    uint32_t next_mem = 0;
    for (int m = 0; m < MEMORY_BITS; m++) {
        float m_acc = 0.0f;
        for (int i = 0; i < NEURONS; i++) {
            if ((l2_out >> i) & 1) m_acc += swarm[id].memory_weights[m][i];
        }
        if (m_acc > 0.0f) next_mem |= (1U << m);
    }
    recurrent_32[id] = next_mem;
    return l2_out;
}

float think_iafz_64(int id, uint64_t sens_lo, uint32_t sens_hi) {
    if (id < 0 || id >= MAX_AGENTS) return 0.0f;
    uint64_t l2 = forward_iafz_64(id, sens_lo, sens_hi);
    float action = 0.0f;
    for (int i = 0; i < NEURONS; i++) {
        if ((l2 >> i) & 1) action += swarm[id].out_action_weights[0][i];
    }
    return action;
}

void think_multi_iafz_64(int id, uint64_t sens_lo, uint32_t sens_hi, float *out) {
    if (!out) return;
    if (id < 0 || id >= MAX_AGENTS) {
        for (int o = 0; o < SALIDAS; o++) out[o] = 0.0f;
        return;
    }
    uint64_t l2 = forward_iafz_64(id, sens_lo, sens_hi);
    for (int o = 0; o < SALIDAS; o++) {
        float a = 0.0f;
        for (int i = 0; i < NEURONS; i++) {
            if ((l2 >> i) & 1) a += swarm[id].out_action_weights[o][i];
        }
        out[o] = a;
    }
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
        if ((uint32_t)xorshift64(tid) < prob) swarm[tid].hidden[i].weights[0] ^= xorshift64(tid);
        if ((uint32_t)xorshift64(tid) < prob) swarm[tid].hidden[i].weights[1] ^= xorshift64(tid);
        if ((uint32_t)xorshift64(tid) < (prob / 2))
            swarm[tid].hidden[i].threshold = 44 + (uint8_t)(xorshift64(tid) % 41);

        // Capa 2
        if ((uint32_t)xorshift64(tid) < prob) swarm[tid].strategy[i].weights[0] ^= xorshift64(tid);
        if ((uint32_t)xorshift64(tid) < prob) swarm[tid].strategy[i].weights[1] ^= xorshift64(tid);
        if ((uint32_t)xorshift64(tid) < (prob / 2))
            swarm[tid].strategy[i].threshold = 44 + (uint8_t)(xorshift64(tid) % 41);
        
        // Accion analogica: las SALIDAS consignas
        for (int o = 0; o < SALIDAS; o++) {
            if ((uint32_t)xorshift64(tid) < prob)
                swarm[tid].out_action_weights[o][i] += random_float(tid) * 0.05f;
        }

        // Memoria Recurrente
        for (int m = 0; m < MEMORY_BITS; m++) {
            if ((uint32_t)xorshift64(tid) < prob) {
                swarm[tid].memory_weights[m][i] += random_float(tid) * 0.05f;
            }
        }
    }
}

void nudge_iafz_64(int sid, int tid, int bits, float paso, uint32_t seed) {
    if (sid < 0 || sid >= MAX_AGENTS || tid < 0 || tid >= MAX_AGENTS) return;
    if (sid != tid) memcpy(&swarm[tid], &swarm[sid], sizeof(Brain64));
    /* Semilla explicita: el que llama decide la secuencia, asi el paso de
       adaptacion es reproducible y se puede auditar despues. */
    rng_state[tid] = (uint64_t)seed * 0x9E3779B97F4A7C15ULL + 1ULL;
    if (rng_state[tid] == 0ULL) rng_state[tid] = 1ULL;

    for (int b = 0; b < bits; b++) {
        uint64_t r = xorshift64(tid);
        int neurona = (int)(r % NEURONS);
        int capa    = (int)((r >> 8) & 1);
        int palabra = (int)((r >> 9) & 1);
        int bit     = (int)((r >> 10) % 64);
        Neuron64 *n = capa ? &swarm[tid].strategy[neurona] : &swarm[tid].hidden[neurona];
        n->weights[palabra] ^= (1ULL << bit);
    }
    if (paso > 0.0f) {
        for (int o = 0; o < SALIDAS; o++) {
            for (int i = 0; i < NEURONS; i++) {
                swarm[tid].out_action_weights[o][i] += random_float(tid) * paso;
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

        for (int o = 0; o < SALIDAS; o++) {
            if (xorshift64(child) % 2 == 0) swarm[child].out_action_weights[o][i] = swarm[p1].out_action_weights[o][i];
            else                            swarm[child].out_action_weights[o][i] = swarm[p2].out_action_weights[o][i];
        }

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
            swarm[id].hidden[i].weights[0] = xorshift64(id);
            swarm[id].hidden[i].weights[1] = xorshift64(id);
            swarm[id].hidden[i].threshold = 44 + (uint8_t)(xorshift64(id) % 41);

            swarm[id].strategy[i].weights[0] = xorshift64(id);
            swarm[id].strategy[i].weights[1] = xorshift64(id);
            swarm[id].strategy[i].threshold = 44 + (uint8_t)(xorshift64(id) % 41);
            
            for (int o = 0; o < SALIDAS; o++) swarm[id].out_action_weights[o][i] = random_float(id);
            
            for(int m = 0; m < MEMORY_BITS; m++) {
                swarm[id].memory_weights[m][i] = random_float(id);
            }
        }
    }
    recurrent_32[id] = 0; 
}

void export_iafz_64(int id, unsigned char *buf) {
    if (id < 0 || id >= MAX_AGENTS || !buf) return;
    memcpy(buf, &swarm[id], sizeof(Brain64));
}

void import_iafz_64(int id, const unsigned char *buf) {
    if (id < 0 || id >= MAX_AGENTS || !buf) return;
    memcpy(&swarm[id], buf, sizeof(Brain64));
    /* La memoria recurrente NO viaja con el genoma: es el eco del vuelo
       anterior, no lo aprendido. Traerla seria arrancar el comportamiento
       nuevo con el contexto del viejo. */
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

/* Activaciones de la última decisión, para el visor. `l1`/`l2` son los
   patrones de disparo de las dos capas binarias (bit i = neurona i prendida);
   `mem` es el estado de memoria recurrente ACTUAL (el que ya se calculó al
   final de la última decisión y se va a usar como contexto en la próxima).
   No cambia nada del cómputo — sólo lee lo que `forward_iafz_64` ya dejó
   guardado. */
void leer_activaciones_iafz_64(int id, uint64_t *l1, uint64_t *l2, uint32_t *mem) {
    if (id < 0 || id >= MAX_AGENTS) {
        if (l1) *l1 = 0;
        if (l2) *l2 = 0;
        if (mem) *mem = 0;
        return;
    }
    if (l1) *l1 = last_l1[id];
    if (l2) *l2 = last_l2[id];
    if (mem) *mem = recurrent_32[id];
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
