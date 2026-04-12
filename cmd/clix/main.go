package main

import (
	"fmt"
	"os"

	"github.com/RyanMerlin/clix/internal/clix"
)

func main() {
	if err := clix.Run(); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
