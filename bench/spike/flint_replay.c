/* Arithmetic spike: replays a trace of polynomial operations (diagcalc-bench --trace) on
 * FLINT's fmpz_mpoly, in graded lexicographic order, and prints the same checksum as
 * spike-replay (replay.rs).
 *
 *   cc -O2 -o flint-replay flint_replay.c -lflint -lgmp
 *   flint-replay TRACE [reuse|fresh|precheck]
 *
 * "reuse" writes every result into one preallocated polynomial (the most favourable case for
 * FLINT); "fresh" initialises and clears a result polynomial per operation, as an engine that
 * stores its results must; "precheck" is "reuse" with the cheap exactness test the prototype
 * uses (a division by x_i - x_j is exact iff the substitution x_i := x_j gives zero) before
 * calling fmpz_mpoly_divides.
 */
#include <flint/fmpz_mpoly.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#define CHUNK 100000
#define MAXV 9

typedef struct {
    int kind; /* 0 mul, 1 add, 2 div */
    int n, i, j;
    fmpz_mpoly_t a, b;
} op_t;

static fmpz_mpoly_ctx_t ctx[MAXV];
static fmpz_mpoly_t diff[MAXV][MAXV][MAXV]; /* x_i - x_j in n variables */
static int precheck;
static fmpz_mpoly_struct gens[MAXV][MAXV];       /* x_v in n variables */

static double now(void) {
    struct timespec t;
    clock_gettime(CLOCK_MONOTONIC, &t);
    return t.tv_sec + 1e-9 * t.tv_nsec;
}

static void read_poly(char **p, int n, fmpz_mpoly_t out, const fmpz_mpoly_ctx_t c) {
    long len = strtol(*p, p, 10);
    ulong e[MAXV];
    fmpz_mpoly_init(out, c);
    for (long t = 0; t < len; t++) {
        slong coef = strtol(*p, p, 10);
        for (int v = 0; v < n; v++) e[v] = strtoul(*p, p, 10);
        fmpz_mpoly_push_term_si_ui(out, coef, e, c);
    }
    fmpz_mpoly_sort_terms(out, c);
    fmpz_mpoly_combine_like_terms(out, c);
}

static uint64_t term_hash(int64_t c, const slong *e, int n) {
    uint64_t h = 0;
    for (int v = 0; v < n; v++) h = h * 131 + (uint64_t)e[v];
    return (uint64_t)c * (h + 1);
}

static uint64_t checksum(const fmpz_mpoly_t p, int n, const fmpz_mpoly_ctx_t c) {
    uint64_t s = 0;
    slong e[MAXV];
    for (slong t = 0; t < fmpz_mpoly_length(p, c); t++) {
        fmpz_mpoly_get_term_exp_si(e, p, t, c);
        s += term_hash(fmpz_mpoly_get_term_coeff_si(p, t, c), e, n);
    }
    return s;
}

/* Returns 1 for an exact division (and sets r), 0 otherwise. */
static int apply(op_t *o, fmpz_mpoly_t r) {
    const fmpz_mpoly_ctx_struct *c = ctx[o->n];
    switch (o->kind) {
    case 0: fmpz_mpoly_mul(r, o->a, o->b, c); return 1;
    case 1: fmpz_mpoly_add(r, o->a, o->b, c); return 1;
    default:
        if (precheck) {
            /* r := a(x_i := x_j) */
            fmpz_mpoly_struct *sub[MAXV];
            for (int v = 0; v < o->n; v++) sub[v] = &gens[o->n][v == o->i ? o->j : v];
            fmpz_mpoly_compose_fmpz_mpoly(r, o->a, sub, c, c);
            if (!fmpz_mpoly_is_zero(r, c)) return 0;
        }
        return fmpz_mpoly_divides(r, o->a, diff[o->n][o->i][o->j], c);
    }
}

static long count[3], failed;
static double secs[3];
static uint64_t sum;
static int fresh;

static void replay(op_t *ops, long m) {
    fmpz_mpoly_t r;
    for (int kind = 0; kind < 3; kind++) {
        double t0 = now();
        if (!fresh) fmpz_mpoly_init(r, ctx[MAXV - 1]);
        for (long k = 0; k < m; k++) {
            if (ops[k].kind != kind) continue;
            if (fresh) fmpz_mpoly_init(r, ctx[ops[k].n]);
            apply(&ops[k], r);
            if (fresh) fmpz_mpoly_clear(r, ctx[ops[k].n]);
            count[kind]++;
        }
        if (!fresh) fmpz_mpoly_clear(r, ctx[MAXV - 1]);
        secs[kind] += now() - t0;
    }
    /* checksum in the same order as replay.rs: muls, adds, divs */
    for (int kind = 0; kind < 3; kind++)
        for (long k = 0; k < m; k++) {
            if (ops[k].kind != kind) continue;
            fmpz_mpoly_init(r, ctx[ops[k].n]);
            if (apply(&ops[k], r))
                sum += checksum(r, ops[k].n, ctx[ops[k].n]);
            else {
                sum += 7;
                failed++;
            }
            fmpz_mpoly_clear(r, ctx[ops[k].n]);
        }
    for (long k = 0; k < m; k++) {
        fmpz_mpoly_clear(ops[k].a, ctx[ops[k].n]);
        if (ops[k].kind != 2) fmpz_mpoly_clear(ops[k].b, ctx[ops[k].n]);
    }
}

int main(int argc, char **argv) {
    if (argc < 2) {
        fprintf(stderr, "usage: flint-replay TRACE [reuse|fresh]\n");
        return 2;
    }
    fresh = argc > 2 && strcmp(argv[2], "fresh") == 0;
    precheck = argc > 2 && strcmp(argv[2], "precheck") == 0;
    for (int n = 1; n < MAXV; n++) {
        fmpz_mpoly_ctx_init(ctx[n], n, ORD_DEGLEX);
        for (int v = 0; v < n; v++) {
            fmpz_mpoly_init(&gens[n][v], ctx[n]);
            fmpz_mpoly_gen(&gens[n][v], v, ctx[n]);
        }
        for (int i = 0; i < n; i++)
            for (int j = 0; j < n; j++) {
                fmpz_mpoly_init(diff[n][i][j], ctx[n]);
                if (i == j) continue;
                fmpz_mpoly_gen(diff[n][i][j], i, ctx[n]);
                fmpz_mpoly_t xj;
                fmpz_mpoly_init(xj, ctx[n]);
                fmpz_mpoly_gen(xj, j, ctx[n]);
                fmpz_mpoly_sub(diff[n][i][j], diff[n][i][j], xj, ctx[n]);
                fmpz_mpoly_clear(xj, ctx[n]);
            }
    }
    FILE *f = fopen(argv[1], "r");
    if (!f) {
        perror(argv[1]);
        return 1;
    }
    op_t *ops = malloc(sizeof(op_t) * CHUNK);
    long m = 0;
    char *line = NULL;
    size_t cap = 0;
    while (getline(&line, &cap, f) > 0) {
        op_t *o = &ops[m];
        char *p = line;
        o->kind = line[0] == 'm' ? 0 : line[0] == 'a' ? 1 : 2;
        p += 4;
        o->n = strtol(p, &p, 10);
        if (o->kind == 2) {
            o->i = strtol(p, &p, 10);
            o->j = strtol(p, &p, 10);
        }
        read_poly(&p, o->n, o->a, ctx[o->n]);
        if (o->kind != 2) read_poly(&p, o->n, o->b, ctx[o->n]);
        if (++m == CHUNK) {
            replay(ops, m);
            m = 0;
        }
    }
    replay(ops, m);
    printf("{\"backend\": \"flint-%s\", \"platform\": \"native\", \"mul\": [%ld, %.4f], "
           "\"add\": [%ld, %.4f], \"div\": [%ld, %.4f], \"failed_divs\": %ld, "
           "\"total_seconds\": %.4f, \"checksum\": \"%016llx\"}\n",
           fresh ? "fresh" : precheck ? "precheck" : "reuse", count[0], secs[0], count[1], secs[1], count[2], secs[2],
           failed, secs[0] + secs[1] + secs[2], (unsigned long long)sum);
    return 0;
}
