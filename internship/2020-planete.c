#include <stdio.h>

typedef struct {
   char* name;
   int radius;
} planet_t;

planet_t solar_sys[8];

planet_t* planet_new(const char* name, int radius);
void planet_free(planet_t* p);
planet_t** read_system(const char*nom_fichier);

int main(int argc, char** argv) {
   planet_t jupiter = { "jupiter", 69911};

   planet_t **planets = read_system("system.txt");

   printf("%s %d\n", jupiter.name, jupiter.radius);
   for (planet_t* curr=planets; cour != NULL; ?????) {
      printf("%s %d\n", ????, ????); // Displays the info of the current planet
   }

   // Free the memory that was allocated by read_system()

   return 0;
}

