using System.Collections;
using System.Collections.Generic;
using UnityEngine;

public class CursorManage : MonoBehaviour
{
    public Texture2D cursorWhite;
    public Texture2D cursorRed;
    public Texture2D cursorGreen;
    public Texture2D cursorYellow;
    public Vector2 highlightVector2 = new Vector2(10, 5);

    // Update is called once per frame
    void Update()
    {
        if (Input.GetMouseButtonDown(0))
            Cursor.SetCursor(cursorRed, highlightVector2, CursorMode.Auto);
        if (Input.GetMouseButtonDown(1))
            Cursor.SetCursor(cursorGreen, highlightVector2, CursorMode.Auto);
        if (Input.GetMouseButtonUp(0) || Input.GetMouseButtonUp(1))
            Cursor.SetCursor(cursorWhite, highlightVector2, CursorMode.Auto);
    }
}
