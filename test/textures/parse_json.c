#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <ctype.h>

#define MAX_LINE 256

int main(void) {
  char line[MAX_LINE];
  FILE *fin;

  if ((fin = fopen("sprites.json", "r")) == NULL) {
    perror("cannot open file");
    exit(1);
  }

  while (fgets(line, sizeof(line), fin)) {
    char *p, *name;
    int x, y, w, h;

    if (strchr(line, '\n') == NULL) {
      fprintf(stderr, "bad line, maybe too long\n");
      exit(1);
    }

    p = line;
    while (*p != '\0' && isspace(*p)) p++;

    if (*p == '{' || *p == '}') continue;
    
    p += strlen("\"");
    name = p;
    while (*p != '\0' && *p != '"') p++;
    *p = '\0';

    p += strlen("\": { \"x\": ");
    x = atoi(p);
    while (*p != '\0' && isdigit(*p)) p++;
    *p = '\0';

    p += strlen(", \"y\": ");
    y = atoi(p);
    while (*p != '\0' && isdigit(*p)) p++;
    *p = '\0';

    p += strlen(", \"w\": ");
    w = atoi(p);
    while (*p != '\0' && isdigit(*p)) p++;
    *p = '\0';

    p += strlen(", \"h\": ");
    h = atoi(p);
    while (*p != '\0' && isdigit(*p)) p++;
    *p = '\0';

    printf("%s %d %d %d %d\n", name, x, y, w, h);
  }

  fclose(fin);

  return 0;
}
