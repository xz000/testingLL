using System.Collections;
using System.Collections.Generic;
using UnityEngine;

public class WeiQiControl : MonoBehaviour
{
    public bool IBlack;
    public Color HeColor;
    public SpriteRenderer srDa;
    public SpriteRenderer srZhong;
    public SpriteRenderer srXiao;
    public WeiQi qipan;

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

    public void pickMe()
    {
        if (IBlack)
            qipan.ToBlack();
        else
            qipan.ToWhite();
    }
}
