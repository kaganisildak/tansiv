#include <stdio.h>
#include <ctype.h> // islower()
#include <unistd.h>
#include <sys/wait.h>

int main(int argc, char *argv[]){  
  int A[2], B[2];
  char c;

  pipe(A); 
  pipe(B);

  if (fork()) {
    if (fork()) {
      close(A[0]); 
      close(B[0]);
      while (read(0,&c,1) == 1) {
        if (islower(c)) {
          write(A[1], &c, 1);
        } else {
          write(B[1], &c, 1);
        }
      }
      close(A[1]); 
      close(B[1]);
      wait(NULL);
      wait(NULL);
    } else {
      dup2(B[0],0);
      close(A[0]); 
      close(A[1]); 
      close(B[0]);
      close(B[1]);
      while (read(0,&c,1) == 1) 
        printf("1: %c\n", c);
    }
  } else {
      dup2(A[0],0);
      close(A[0]);
      close(A[1]);
      close(B[0]); 
      close(B[1]); 
      while (read(0,&c,1) == 1) 
        printf("2: %c\n", c);
  }
  return 0;
}
