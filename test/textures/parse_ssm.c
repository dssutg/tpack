#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <ctype.h>

#define MAX_LINE 256

int main(void) {
  char line[MAX_LINE], name[MAX_LINE];
  FILE *fin;

  if ((fin = fopen("sprites.ssm", "r")) == NULL) {
    perror("cannot open file");
    exit(1);
  }

  while (fgets(line, sizeof(line), fin)) {
    int x, y, w, h;

    if (strchr(line, '\n') == NULL) {
      fprintf(stderr, "bad line, maybe too long\n");
      exit(1);
    }

    sscanf(line, "S %s %d %d %d %d", name, &x, &y, &w, &h);

    printf("name=%s, x=%d, y=%d, w=%d, h=%d\n", name, x, y, w, h);
  }

  fclose(fin);

  return 0;
}
