.intel_syntax noprefix

# All helper functions are called with RSP misaligned to 16-byte boundary
# So when return address is pushed, they are properly aligned

.global helper_trap
.extern trap
helper_trap:
    # RDI -> context
    mov rdi, rbp
    # We use EBX to store the instruction offset within the current basic block
    # Lower 16 bits are PC offset and upper 16 bits are INSTRET offset.
    movsx rax, bx
    add [rbp + 0x100], rax
    shr ebx, 16
    movsx rax, bx
    add [rbp + 0x108], rax
    call trap
    # Pop out trapping PC
    add rsp, 8
    ret

# RSI -> vaddr
.global helper_read_misalign
helper_read_misalign:
    # Set scause and stval
    mov qword ptr [rbp + 32 * 8 + 16], 4
    mov [rbp + 32 * 8 + 24], rsi
    call helper_trap

# RSI -> vaddr
.global helper_write_misalign
helper_write_misalign:
    # Set scause and stval
    mov qword ptr [rbp + 32 * 8 + 16], 5
    mov [rbp + 32 * 8 + 24], rsi
    call helper_trap

.extern riscv_step
.global helper_step
helper_step:
    mov rdi, rbp
    call riscv_step
    test al, al
    jnz helper_trap
    ret

.global helper_translate_cache_miss
.extern translate_cache_miss
helper_translate_cache_miss:
    mov rdi, rbp
    call translate_cache_miss
    test al, al
    jnz helper_trap
    mov rsi, rdx
    ret

.global helper_icache_miss
.extern insn_translate_cache_miss
helper_icache_miss:
    mov rdi, rbp
    call insn_translate_cache_miss
    test al, al
    jnz helper_trap
    mov rsi, rdx
    ret

.global helper_icache_cross_miss
.extern icache_cross_miss
helper_icache_cross_miss:
    mov rdi, rbp
    # The instruction to translate
    movzx eax, word ptr [rsi]
    shl eax, 16
    or ecx, eax
    # Load the return address
    mov rdx, [rsp]
    movsx rax, dword ptr [rdx + 1]
    lea rdx, [rdx+rax+5]
    call icache_cross_miss
    ret

.global helper_icache_wrong
.extern find_block_and_patch
helper_icache_wrong:
    mov rdi, rbp
    # Load return address into RSI
    mov rsi, [rsp]
    call find_block_and_patch
    ret


.global helper_icache_patch2
.extern find_block_and_patch2
helper_icache_patch2:
    mov rdi, rbp
    # Load return address into RSI
    mov rsi, [rsp]
    call find_block_and_patch2
    sub qword ptr [rsp], 5
    ret

.global helper_check_interrupt
.extern check_interrupt
helper_check_interrupt:
    mov rdi, rbp
    sub rsp, 8
    call check_interrupt
    test al, al
    jnz 1f
    add rsp, 8
    ret
1:
    # If check_interrupt returns Err(()), it indicates that we would like to shutdown.
    # We therefore need to return from fiber_interp_run.
    add rsp, 24
    ret

.global helper_san_fail
helper_san_fail:
    ud2

.extern find_block
.global fiber_interp_run
fiber_interp_run:
    mov rdi, rbp
    sub rsp, 8
    call find_block
    call rax
    add rsp, 8
    jmp fiber_interp_run
