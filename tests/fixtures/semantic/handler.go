package handler

import "context"

// UserHandler handles HTTP requests for user operations.
type UserHandler struct {
	service UserService
}

// UserService is the business logic interface.
type UserService interface {
	Save(ctx context.Context, name string) error
	FindByName(ctx context.Context, name string) (*User, error)
}

// User is the domain model.
type User struct {
	ID   int64
	Name string
}

// NewUserHandler constructs a handler.
func NewUserHandler(svc UserService) *UserHandler {
	return &UserHandler{service: svc}
}

// HandleCreate handles POST /users.
func (h *UserHandler) HandleCreate(ctx context.Context, name string) error {
	return h.service.Save(ctx, name)
}
