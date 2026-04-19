" Filetype detection for cljrs. Loaded by nvim before any plugin code so
" the filetype gets set even if the user hasn't called setup() yet.
autocmd BufRead,BufNewFile *.cljrs set filetype=cljrs
