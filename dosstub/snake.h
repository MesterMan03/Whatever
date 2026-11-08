#ifndef SNAKE_H
#define SNAKE_H

typedef struct
{
    int x;
    int y;
} position;

typedef enum
{
    SNAKE_UP,
    SNAKE_RIGHT,
    SNAKE_DOWN,
    SNAKE_LEFT,
    SNAKE_NO_TURN
} snake_direction;

typedef enum
{
    SNAKE_PLAYING,
    SNAKE_CRASHED,
    SNAKE_WON,
    SNAKE_QUIT
} snake_result;

typedef struct
{
    int screen_w;
    int screen_h;
    int capacity;
    int length;
    position food;
    position *snake_pos;
    unsigned char *occupied;
} game_state;

game_state *initialize(void);
snake_result step_snake(game_state *game, snake_direction direction);
void destroy_game(game_state *game);
void render_board(const game_state *game);

#endif // SNAKE_H
