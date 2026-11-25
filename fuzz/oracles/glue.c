/* Small C glue so the Rust fuzz targets don't have to mirror C structs. */
#include <plist/plist.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "pair_ap/pair-tlv.h"

/*
 * Parse with pair_ap. Returns <0 on parse error. On success, for each of the
 * 256 types writes the length of the FIRST item of that type into lens[type]
 * (or -1 if absent) and copies its value into vals + type*65536 (truncated).
 * Also returns the number of items in the list.
 */
int pair_ap_tlv_parse(const uint8_t *buf, size_t len, long *lens, uint8_t *vals) {
  pair_tlv_values_t *values = pair_tlv_new();
  if (!values)
    return -100;
  int ret = pair_tlv_parse(buf, len, values);
  if (ret < 0) {
    pair_tlv_free(values);
    return ret;
  }
  for (int t = 0; t < 256; t++)
    lens[t] = -1;
  int count = 0;
  for (pair_tlv_t *t = values->head; t; t = t->next) {
    count++;
    if (lens[t->type] == -1) {
      lens[t->type] = (long)t->size;
      size_t n = t->size < 65536 ? t->size : 65536;
      if (n)
        memcpy(vals + (size_t)t->type * 65536, t->value, n);
    }
  }
  pair_tlv_free(values);
  return count;
}

/*
 * Parse a binary plist with libplist and render it in a canonical text form
 * that the Rust side reproduces from its own PlistValue:
 *   b0/b1, i<decimal> (signed if negative else unsigned), r<f64 bits hex>,
 *   s<hex bytes>, x<hex bytes> (data), t (date, value not compared),
 *   u<decimal> (UID), n (null: libplist only), a[..,..],
 *   d{<key hex>=<val>,..} with keys sorted.
 * Returns a malloc'd string, or NULL if libplist rejects the input.
 */
typedef struct {
  char *buf;
  size_t len, cap;
} sb_t;

static void sb_put(sb_t *b, const char *s, size_t n) {
  if (b->len + n + 1 > b->cap) {
    b->cap = (b->len + n + 1) * 2;
    b->buf = realloc(b->buf, b->cap);
  }
  memcpy(b->buf + b->len, s, n);
  b->len += n;
  b->buf[b->len] = 0;
}
static void sb_str(sb_t *b, const char *s) { sb_put(b, s, strlen(s)); }
static void sb_hex(sb_t *b, const unsigned char *p, size_t n) {
  static const char *h = "0123456789abcdef";
  for (size_t i = 0; i < n; i++) {
    char c[2] = {h[p[i] >> 4], h[p[i] & 15]};
    sb_put(b, c, 2);
  }
}

typedef struct {
  char *key_hex;
  char *val;
} kv_t;
static int kv_cmp(const void *a, const void *b) {
  return strcmp(((const kv_t *)a)->key_hex, ((const kv_t *)b)->key_hex);
}

static void canon(sb_t *b, plist_t n, int depth) {
  char tmp[64];
  if (depth > 512) {
    sb_str(b, "!depth");
    return;
  }
  switch (plist_get_node_type(n)) {
  case PLIST_BOOLEAN: {
    uint8_t v = 0;
    plist_get_bool_val(n, &v);
    sb_str(b, v ? "b1" : "b0");
    break;
  }
  case PLIST_INT: {
    if (plist_int_val_is_negative(n)) {
      int64_t v;
      plist_get_int_val(n, &v);
      snprintf(tmp, sizeof tmp, "i%lld", (long long)v);
    } else {
      uint64_t v;
      plist_get_uint_val(n, &v);
      snprintf(tmp, sizeof tmp, "i%llu", (unsigned long long)v);
    }
    sb_str(b, tmp);
    break;
  }
  case PLIST_REAL: {
    double v;
    uint64_t bits;
    plist_get_real_val(n, &v);
    memcpy(&bits, &v, 8);
    snprintf(tmp, sizeof tmp, "r%016llx", (unsigned long long)bits);
    sb_str(b, tmp);
    break;
  }
  case PLIST_STRING: {
    uint64_t len = 0;
    const char *p = plist_get_string_ptr(n, &len);
    sb_str(b, "s");
    sb_hex(b, (const unsigned char *)p, p ? strlen(p) : 0); /* up to first NUL; see lib.rs */
    break;
  }
  case PLIST_DATA: {
    uint64_t len = 0;
    const char *p = plist_get_data_ptr(n, &len);
    sb_str(b, "x");
    sb_hex(b, (const unsigned char *)p, len);
    break;
  }
  case PLIST_DATE:
    sb_str(b, "t");
    break;
  case PLIST_UID: {
    uint64_t v;
    plist_get_uid_val(n, &v);
    snprintf(tmp, sizeof tmp, "u%llu", (unsigned long long)v);
    sb_str(b, tmp);
    break;
  }
  case PLIST_ARRAY: {
    sb_str(b, "a[");
    uint32_t sz = plist_array_get_size(n);
    for (uint32_t i = 0; i < sz; i++) {
      if (i)
        sb_str(b, ",");
      canon(b, plist_array_get_item(n, i), depth + 1);
    }
    sb_str(b, "]");
    break;
  }
  case PLIST_DICT: {
    uint32_t sz = plist_dict_get_size(n);
    kv_t *kvs = calloc(sz ? sz : 1, sizeof(kv_t));
    plist_dict_iter it = NULL;
    plist_dict_new_iter(n, &it);
    uint32_t i = 0;
    for (;;) {
      char *key = NULL;
      plist_t v = NULL;
      plist_dict_next_item(n, it, &key, &v);
      if (!v || i >= sz) {
        free(key);
        break;
      }
      sb_t kb = {0}, vb = {0};
      sb_put(&kb, "", 0);
      sb_hex(&kb, (const unsigned char *)key, strlen(key));
      sb_put(&vb, "", 0);
      canon(&vb, v, depth + 1);
      kvs[i].key_hex = kb.buf;
      kvs[i].val = vb.buf;
      i++;
      free(key);
    }
    free(it);
    qsort(kvs, i, sizeof(kv_t), kv_cmp);
    sb_str(b, "d{");
    for (uint32_t j = 0; j < i; j++) {
      if (j)
        sb_str(b, ",");
      sb_str(b, kvs[j].key_hex);
      sb_str(b, "=");
      sb_str(b, kvs[j].val);
      free(kvs[j].key_hex);
      free(kvs[j].val);
    }
    free(kvs);
    sb_str(b, "}");
    break;
  }
  case PLIST_NULL:
    sb_str(b, "n");
    break;
  default:
    sb_str(b, "?");
  }
}

char *libplist_bin_canonical(const char *buf, uint32_t len) {
  plist_t root = NULL;
  plist_from_bin(buf, len, &root);
  if (!root)
    return NULL;
  sb_t b = {0};
  sb_put(&b, "", 0);
  canon(&b, root, 0);
  plist_free(root);
  return b.buf;
}

#include "shairport-sync/alac.h"

/*
 * Decode one ALAC packet with David Hammerton's decoder (Shairport Sync's
 * default), configured from the fmtp fields exactly as player.c's
 * init_alac_decoder does for 16-bit stereo, 352 frames per packet.
 * `in` must have some slack after `len`: this decoder does not bounds-check.
 * Returns the number of bytes written to `out`.
 */
int hammerton_alac_decode(const int32_t fmtp[12], unsigned char *in, void *out) {
  alac_file *alac = alac_create(16, 2);
  if (!alac)
    return -1;
  alac->setinfo_max_samples_per_frame = 352;
  alac->setinfo_7a = fmtp[2];
  alac->setinfo_sample_size = 16;
  alac->setinfo_rice_historymult = fmtp[4];
  alac->setinfo_rice_initialhistory = fmtp[5];
  alac->setinfo_rice_kmodifier = fmtp[6];
  alac->setinfo_7f = fmtp[7];
  alac->setinfo_80 = fmtp[8];
  alac->setinfo_82 = fmtp[9];
  alac->setinfo_86 = fmtp[10];
  alac->setinfo_8a_rate = fmtp[11];
  alac_allocate_buffers(alac);
  int size = 352 * 4; /* in: buffer capacity, as player.c sets it */
  alac_decode_frame(alac, in, out, &size);
  alac_free(alac);
  return size;
}
