package clix

import (
	"bufio"
	"encoding/json"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

func appendReceipt(dir string, receipt map[string]any) error {
	if err := ensureDir(dir); err != nil {
		return err
	}
	file := filepath.Join(dir, "receipts.jsonl")
	f, err := os.OpenFile(file, os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0o644)
	if err != nil {
		return err
	}
	defer f.Close()
	b, err := json.Marshal(receipt)
	if err != nil {
		return err
	}
	_, err = f.WriteString(string(b) + "\n")
	return err
}

func readReceipts(dir string) ([]map[string]any, error) {
	file := filepath.Join(dir, "receipts.jsonl")
	if !fileExists(file) {
		return nil, nil
	}
	f, err := os.Open(file)
	if err != nil {
		return nil, err
	}
	defer f.Close()
	var out []map[string]any
	scanner := bufio.NewScanner(f)
	for scanner.Scan() {
		line := strings.TrimSpace(scanner.Text())
		if line == "" {
			continue
		}
		var item map[string]any
		if err := json.Unmarshal([]byte(line), &item); err == nil {
			out = append(out, item)
		}
	}
	sort.SliceStable(out, func(i, j int) bool {
		return toString(out[i]["createdAt"]) > toString(out[j]["createdAt"])
	})
	return out, scanner.Err()
}

func findReceipt(dir, id string) (map[string]any, error) {
	receipts, err := readReceipts(dir)
	if err != nil {
		return nil, err
	}
	for _, r := range receipts {
		if toString(r["id"]) == id {
			return r, nil
		}
	}
	return nil, nil
}

func toString(v any) string {
	switch t := v.(type) {
	case string:
		return t
	case nil:
		return ""
	default:
		b, _ := json.Marshal(t)
		return string(b)
	}
}
