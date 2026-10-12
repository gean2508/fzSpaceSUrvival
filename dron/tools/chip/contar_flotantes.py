import sys,struct
from unicorn import *
from unicorn.riscv_const import *
from elftools.elf.elffile import ELFFile
obj,gen=sys.argv[1],sys.argv[2]
e=ELFFile(open(obj,'rb')); text=e.get_section_by_name('.text').data()
sym=[s for s in e.get_section_by_name('.symtab').iter_symbols() if s.name=='pensar_abordo'][0]['st_value']
mu=Uc(UC_ARCH_RISCV, UC_MODE_RISCV32)
CODE,BR,ST,STK,RET=0x10000,0x20000,0x30000,0x80000,0x9000
for a,sz in [(CODE,0x10000),(ST,0x1000),(STK-0x10000,0x10000),(RET,0x1000),(BR,0x4000)]: mu.mem_map(a,sz)
mu.mem_write(CODE,text); mu.mem_write(BR,open(gen,'rb').read())
mu.reg_write(UC_RISCV_REG_MSTATUS, mu.reg_read(UC_RISCV_REG_MSTATUS)|(3<<13))
fadd=[0]; fcmp=[0]; tot=[0]
def h(uc,addr,size,ud):
    tot[0]+=1
    if size==4:
        w=struct.unpack('<I',uc.mem_read(addr,4))[0]
        if (w&0x7f)==0x53:
            f7=w>>25
            if f7==0x00: fadd[0]+=1
            elif f7==0x50: fcmp[0]+=1
mu.hook_add(UC_HOOK_CODE,h)
x=0x9E3779B97F4A7C15; M=(1<<64)-1
def xs():
    global x
    x^=(x<<13)&M; x^=x>>7; x^=(x<<17)&M; return x
mem=0
for i in range(50):
    lo=xs(); hi=xs()&0xffffffff
    mu.mem_write(ST,struct.pack('<QII4f',lo,hi,mem,0,0,0,0))
    mu.reg_write(UC_RISCV_REG_A0,BR); mu.reg_write(UC_RISCV_REG_A1,ST); mu.reg_write(UC_RISCV_REG_SP,STK-16); mu.reg_write(UC_RISCV_REG_RA,RET)
    mu.emu_start(CODE+sym,RET)
    mem=struct.unpack('<I',mu.mem_read(ST+12,4))[0]
print(f"por decisión: {tot[0]/50:.0f} instrucciones, {fadd[0]/50:.0f} sumas flotantes, {fcmp[0]/50:.0f} comparaciones flotantes")
