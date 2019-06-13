using System.Collections;
using System.Collections.Generic;
using UnityEngine;

public class WeiQiControl : MonoBehaviour
{
    public Color HeColor;
    public SpriteRenderer srDa;
    public SpriteRenderer srZhong;
    public SpriteRenderer srXiao;

    void Start()
    {
        srDa.color = HeColor;
    }

    private void OnMouseEnter()
    {
        srDa.color = Color.red;
    }

    private void OnMouseExit()
    {
        srDa.color = HeColor;
    }
}
