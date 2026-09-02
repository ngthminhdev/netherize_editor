// netherize-leetcode id=217 slug=contains-duplicate
// Contains Duplicate — https://leetcode.com/problems/contains-duplicate/

package main

import (
    "encoding/json"
    "os"
)

func containsDuplicate(nums []int) bool {
    seen := make(map[int]bool)
    for _, num := range nums {
        if seen[num] {
            return true
        }
        seen[num] = true
    }
    return false
}

type inputParams struct {
    Nums []int `json:"nums"`
}

func solve() error {
    var params inputParams
    if err := json.NewDecoder(os.Stdin).Decode(&params); err != nil {
        return err
    }
    result := containsDuplicate(params.Nums)
    return json.NewEncoder(os.Stdout).Encode(result)
}

func main() {
    if err := solve(); err != nil {
        panic(err)
    }
}