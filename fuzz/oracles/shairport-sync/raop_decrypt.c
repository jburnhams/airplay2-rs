/*
 * RAOP (AirPlay 1) audio payload decryption, transcribed from Shairport Sync
 * player.c (commit 7ce90d89, openssl_aes_decrypt_cbc and the ALAC_44100_S16_2
 * branch of the packet handler). Only the glue is new: the function takes the
 * key and IV as arguments instead of reading them from rtsp_conn_info.
 *
 * Copyright (c) James Laird 2011, 2013; Mike Brady 2014--2026.
 * MIT licence: see the header of player.c in Shairport Sync, reproduced in
 * ../README.md.
 */
#include <openssl/evp.h>
#include <stdint.h>
#include <string.h>

static int openssl_aes_decrypt_cbc(unsigned char *ciphertext, int ciphertext_len,
                                   unsigned char *key, unsigned char *iv,
                                   unsigned char *plaintext) {
  EVP_CIPHER_CTX *ctx;
  int len;
  int plaintext_len = 0;
  ctx = EVP_CIPHER_CTX_new();
  if (ctx != NULL) {
    if (EVP_DecryptInit_ex(ctx, EVP_aes_128_cbc(), NULL, key, iv) == 1) {
      EVP_CIPHER_CTX_set_padding(ctx, 0);
      if (EVP_DecryptUpdate(ctx, plaintext, &len, ciphertext, ciphertext_len) == 1) {
        plaintext_len = len;
        if (EVP_DecryptFinal_ex(ctx, plaintext + len, &len) == 1)
          plaintext_len += len;
      }
    }
    EVP_CIPHER_CTX_free(ctx);
  }
  return plaintext_len;
}

/* Decrypt one RTP audio payload of `len` bytes into `out` (also `len` bytes). */
void shairport_raop_decrypt(const uint8_t *aeskey, const uint8_t *aesiv, const uint8_t *data,
                            int len, uint8_t *out) {
  unsigned char key[16];
  unsigned char iv[16];
  int aeslen = len & ~0xf;
  memcpy(key, aeskey, sizeof(key));
  memcpy(iv, aesiv, sizeof(iv));
  openssl_aes_decrypt_cbc((unsigned char *)data, aeslen, key, iv, out);
  memcpy(out + aeslen, data + aeslen, len - aeslen);
}
