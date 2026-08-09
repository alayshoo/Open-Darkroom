/**
 * Shared open state for the title bar's menu buttons.
 *
 * Windows 11 hands the whole bar over to the pointer the moment one menu is
 * open: hovering a sibling button switches to its menu without a second click.
 * That needs one piece of state above the individual buttons, since a button
 * has to know whether *any* menu is open before deciding what a hover means.
 */

let openId = $state<string | null>(null);
/* True while a menu opened by hover-switching rather than by a click, so the
   incoming dropdown can skip its entrance animation the way the native bar
   does — the bar is already open, only the panel under it changes. */
let switching = $state(false);

export const menuBar = {
    isOpen(id: string) {
        return openId === id;
    },

    get switching() {
        return switching;
    },

    toggle(id: string) {
        switching = false;
        openId = openId === id ? null : id;
    },

    /** A hover only takes over while the bar is already open. */
    hover(id: string) {
        if (openId === null || openId === id) return;
        switching = true;
        openId = id;
    },

    close() {
        switching = false;
        openId = null;
    },
};
