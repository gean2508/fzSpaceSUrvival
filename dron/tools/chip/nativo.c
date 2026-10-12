#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
typedef struct { uint64_t lo; uint32_t hi; uint32_t mem; float out[4]; } E;
void pensar_abordo(const void*, E*);
int main(int c, char**v){ FILE*f=fopen(v[1],"rb"); static unsigned char b[11392]; fread(b,1,11392,f); fclose(f);
 E e={0}; uint64_t x=0x9E3779B97F4A7C15ULL;
 for(int i=0;i<50;i++){ x^=x<<13;x^=x>>7;x^=x<<17; e.lo=x; x^=x<<13;x^=x>>7;x^=x<<17; e.hi=(uint32_t)x;
   pensar_abordo(b,&e); printf("%08x %08x %08x %08x %08x\n", e.mem, *(uint32_t*)&e.out[0],*(uint32_t*)&e.out[1],*(uint32_t*)&e.out[2],*(uint32_t*)&e.out[3]); } }
