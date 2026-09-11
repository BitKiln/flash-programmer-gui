/* Minimal bare-register test firmware for NUCLEO-U575ZI-Q (STM32U575ZI).
   Blinks LD1 (green, PC7). No HAL, no libc. */

#include <stdint.h>

#define RCC_BASE      0x46020C00u
#define RCC_AHB2ENR1  (*(volatile uint32_t *)(RCC_BASE + 0x8Cu))

#define GPIOC_BASE    0x42020800u
#define GPIOC_MODER   (*(volatile uint32_t *)(GPIOC_BASE + 0x00u))
#define GPIOC_BSRR    (*(volatile uint32_t *)(GPIOC_BASE + 0x18u))

#define LED_PIN 7u

/* Initialised data, so the linker emits a second PT_LOAD whose physical
   address (flash) differs from its virtual address (RAM). This is what makes
   the ELF parser's use of p_paddr observable. */
volatile uint32_t blink_count = 0xA5A5A5A5u;
volatile uint32_t marker = 0xDEADBEEFu;

/* Padding so the image is comparable in size to the H753 test firmware. */
const uint32_t filler[4800] __attribute__((used)) = { 0x11223344u };

static void delay(volatile uint32_t n)
{
    while (n--) {
        __asm__ volatile("nop");
    }
}

int main(void)
{
    RCC_AHB2ENR1 |= (1u << 2);            /* GPIOCEN */
    GPIOC_MODER &= ~(3u << (LED_PIN * 2));
    GPIOC_MODER |=  (1u << (LED_PIN * 2)); /* output */

    for (;;) {
        GPIOC_BSRR = (1u << LED_PIN);      /* set */
        delay(400000u);
        GPIOC_BSRR = (1u << (LED_PIN + 16)); /* reset */
        delay(400000u);
        blink_count++;
    }
}
