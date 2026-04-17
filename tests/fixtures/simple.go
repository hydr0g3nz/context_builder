// Package userservice provides user management functionality.
package userservice

import "context"

// User represents a system user.
type User struct {
	ID    int64
	Name  string
	Email string
}

// UserStore defines the persistence interface.
type UserStore interface {
	Save(ctx context.Context, u *User) error
	FindByID(ctx context.Context, id int64) (*User, error)
}

// UserService handles business logic for users.
type UserService struct {
	store UserStore
}

// NewUserService creates a new UserService.
func NewUserService(store UserStore) *UserService {
	return &UserService{store: store}
}

// Save persists a user.
func (s *UserService) Save(ctx context.Context, u *User) error {
	return s.store.Save(ctx, u)
}

// findByEmail is a private helper.
func (s *UserService) findByEmail(email string) *User {
	return nil
}

// ErrorCode is a type alias for int.
type ErrorCode int
