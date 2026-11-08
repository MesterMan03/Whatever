#include <conio.h>
#include <graph.h>
#include <i86.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>
#include "snake.h"

// I have absolutely no idea why I made this
// I guess I wanted to practice the C skills from my CS classes lol

enum
{
    KEY_ENTER = 13,
    KEY_ESCAPE = 27,
    KEY_EXTENDED = 0x100,
    KEY_UP = KEY_EXTENDED | 0x48,
    KEY_LEFT = KEY_EXTENDED | 0x4b,
    KEY_RIGHT = KEY_EXTENDED | 0x4d,
    KEY_DOWN = KEY_EXTENDED | 0x50
};

static int read_key(void)
{
    int key = getch();

    if (key == 0) {
        key = getch();
        if (key != EOF) {
            return KEY_EXTENDED | key;
        }
    }
    return key;
}

static snake_direction direction_for_key(int key)
{
    switch (key) {
    case KEY_UP:
    case 'w':
    case 'W':
        return SNAKE_UP;
    case KEY_RIGHT:
    case 'd':
    case 'D':
        return SNAKE_RIGHT;
    case KEY_DOWN:
    case 's':
    case 'S':
        return SNAKE_DOWN;
    case KEY_LEFT:
    case 'a':
    case 'A':
        return SNAKE_LEFT;
    default:
        return SNAKE_NO_TURN;
    }
}

static snake_result play(game_state *game)
{
    // Watcom's DOS clock reads the BIOS timer.
    const clock_t move_interval = CLOCKS_PER_SEC / 6;
    clock_t last_move = clock();
    snake_direction heading = SNAKE_RIGHT;
    snake_direction next_heading = heading;
    snake_result result = SNAKE_PLAYING;

    while (result == SNAKE_PLAYING) {
        clock_t elapsed;

        if (kbhit()) {
            int key = read_key();
            snake_direction turn = direction_for_key(key);

            if (key == KEY_ESCAPE || key == EOF) {
                return SNAKE_QUIT;
            }
            // Accept one turn per move and reject the opposite heading.
            if (next_heading == heading && turn != SNAKE_NO_TURN &&
                turn != (heading + 2) % 4) {
                next_heading = turn;
            }
        }

        elapsed = clock() - last_move;
        if (elapsed >= move_interval) {
            // Carry ordinary timer rounding forward and discard missed moves after a stall.
            last_move += elapsed < 2 * move_interval ? move_interval : elapsed;
            heading = next_heading;
            result = step_snake(game, heading);
        } else {
            delay(10);
        }
    }
    return result;
}

int main(void)
{
    game_state *game;
    snake_result result;
    short previous_cursor;
    short previous_wrap;
    int key;
    int score;

    printf("Sorry mate, your computer is either 30 years too old or you just really like hunting for easter eggs.\n");
    printf("Well unfortunately you cannot play the game you downloaded, but I have something fun that could interest you!\n\n");
    printf("Steer with the arrow keys or WASD. Eat * to grow.\n");
    printf("Cross any edge to wrap around.\n");
    printf("Press ENTER to start or ESC to quit.\n");
    puts("\nDOS runtime source under the Sybase Open Watcom Public License");
    puts("https://github.com/open-watcom/open-watcom-v2");
    fflush(stdout);

    do {
        key = read_key();
        if (key == KEY_ESCAPE || key == EOF) {
            return EXIT_SUCCESS;
        }
    } while (key != KEY_ENTER);

    srand((unsigned int)time(NULL));
    game = initialize();
    if (game == NULL) {
        fprintf(stderr, "Failed to initialize game\n");
        return EXIT_FAILURE;
    }

    previous_cursor = _displaycursor(_GCURSOROFF);
    previous_wrap = _wrapon(_GWRAPOFF);
    _clearscreen(_GCLEARSCREEN);
    render_board(game);
    result = play(game);
    score = game->length - 1;

    destroy_game(game); // no memory leaks here
    _clearscreen(_GCLEARSCREEN);
    _wrapon(previous_wrap);
    _displaycursor(previous_cursor);

    if (result == SNAKE_WON) {
        printf("You filled the board! Score: %d\n", score);
    } else if (result == SNAKE_CRASHED) {
        printf("Game over! Score: %d\n", score);
    }
    return EXIT_SUCCESS;
}
