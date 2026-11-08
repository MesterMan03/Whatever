#include <graph.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "snake.h"

static int cell_index(const game_state *game, position cell)
{
    return (cell.y - 1) * (game->screen_w - 2) + cell.x - 1;
}

static int same_position(position a, position b)
{
    return a.x == b.x && a.y == b.y;
}

static int wrap_coordinate(int coordinate, int last_cell)
{
    if (coordinate < 1) {
        return last_cell;
    }
    if (coordinate > last_cell) {
        return 1;
    }
    return coordinate;
}

static void place_food(game_state *game)
{
    int cell;
    int width = game->screen_w - 2;
    int free_cells_to_skip = rand() % (game->capacity - game->length);

    for (cell = 0; cell < game->capacity; ++cell) {
        if (game->occupied[cell]) {
            continue;
        }
        if (free_cells_to_skip == 0) {
            game->food.x = cell % width + 1;
            game->food.y = cell / width + 1;
            return;
        }
        --free_cells_to_skip;
    }
}

static void draw_cell(int x, int y, char symbol)
{
    char text[2];

    text[0] = symbol;
    text[1] = '\0';
    _settextposition(y + 1, x + 1);
    _outtext(text);
}

static int render_score(const game_state *game)
{
    int score_end;
    char score_text[32];

    sprintf(score_text, " SCORE: %-5d ", game->length - 1);
    score_end = 3 + (int)strlen(score_text) - 1;
    if (score_end < game->screen_w) {
        _settextposition(1, 3);
        _outtext(score_text);
    }
    return score_end;
}

void render_board(const game_state *game)
{
    static char title[] = " WHATEVER SNAKE ";
    int x, y;
    int score_end;
    int title_length = sizeof(title) - 1;
    int title_x = (game->screen_w - title_length) / 2 + 1;

    for (x = 1; x < game->screen_w - 1; ++x) {
        draw_cell(x, 0, '-');
        draw_cell(x, game->screen_h - 1, '-');
    }
    for (y = 1; y < game->screen_h - 1; ++y) {
        draw_cell(0, y, '|');
        draw_cell(game->screen_w - 1, y, '|');
    }
    draw_cell(0, 0, '+');
    draw_cell(game->screen_w - 1, 0, '+');
    draw_cell(0, game->screen_h - 1, '+');
    draw_cell(game->screen_w - 1, game->screen_h - 1, '+');

    score_end = render_score(game);
    if (title_x > score_end && title_x + title_length <= game->screen_w) {
        _settextposition(1, title_x);
        _outtext(title);
    }
    draw_cell(game->snake_pos[0].x, game->snake_pos[0].y, '@');
    draw_cell(game->food.x, game->food.y, '*');
}

snake_result step_snake(game_state *game, snake_direction direction)
{
    static const position steps[] = {
        { 0, -1 }, { 1, 0 }, { 0, 1 }, { -1, 0 }
    };
    position head = game->snake_pos[0];
    position tail = game->snake_pos[game->length - 1];
    int grows;
    int next_cell;

    head.x = wrap_coordinate(head.x + steps[direction].x, game->screen_w - 2);
    head.y = wrap_coordinate(head.y + steps[direction].y, game->screen_h - 2);

    grows = same_position(head, game->food);
    next_cell = cell_index(game, head);
    // On an ordinary move the tail vacates its cell before the head enters.
    if (game->occupied[next_cell] && (grows || !same_position(head, tail))) {
        return SNAKE_CRASHED;
    }

    if (grows) {
        ++game->length;
    } else {
        game->occupied[cell_index(game, tail)] = 0;
        draw_cell(tail.x, tail.y, ' ');
    }
    memmove(game->snake_pos + 1, game->snake_pos, (game->length - 1) * sizeof(position));
    game->snake_pos[0] = head;
    game->occupied[next_cell] = 1;

    if (game->length > 1) {
        draw_cell(game->snake_pos[1].x, game->snake_pos[1].y, '#');
    }
    draw_cell(head.x, head.y, '@');
    if (grows) {
        render_score(game);
        if (game->length == game->capacity) {
            return SNAKE_WON;
        }
        place_food(game);
        draw_cell(game->food.x, game->food.y, '*');
    }
    return SNAKE_PLAYING;
}

game_state *initialize(void)
{
    struct videoconfig vc;
    game_state *game;
    unsigned long capacity;
    size_t bytes_needed;
    int screen_w;
    int screen_h;

    _getvideoconfig(&vc);
    screen_w = vc.numtextcols;
    screen_h = vc.numtextrows;
    if (screen_w < 4 || screen_h < 4) {
        return NULL;
    }

    capacity = (unsigned long)(screen_w - 2) * (screen_h - 2);
    if (capacity > INT_MAX ||
        capacity > ((size_t)-1 - sizeof(game_state)) / (sizeof(position) + 1)) {
        return NULL;
    }
    bytes_needed = sizeof(game_state) + (size_t)capacity * (sizeof(position) + 1);
    game = calloc(1, bytes_needed);
    if (game == NULL) {
        return NULL;
    }

    game->screen_w = screen_w;
    game->screen_h = screen_h;
    game->capacity = (int)capacity;
    game->length = 1;
    game->snake_pos = (position *)(game + 1);
    game->occupied = (unsigned char *)(game->snake_pos + game->capacity);
    game->snake_pos[0].x = (screen_w - 1) / 2;
    game->snake_pos[0].y = (screen_h - 1) / 2;
    game->occupied[cell_index(game, game->snake_pos[0])] = 1;
    place_food(game);
    return game;
}

void destroy_game(game_state *game)
{
    free(game);
}
