using System.Collections;
using System.Collections.Generic;
using UnityEngine;

public class WeiQi : MonoBehaviour
{
    bool nowBlack = false;
    public GameObject qizi;
    public static Color PanColor;
    // Start is called before the first frame update
    void Start()
    {
        PanColor = GetComponent<SpriteRenderer>().color;
        chushihuaQi();
    }

    void Update()
    {
        if (Input.GetMouseButtonDown(0))
            CatchHim();
    }

    void chushihuaQi()
    {

        for (int i = -9; i < 10; i++)
        {
            for (int j = -9; j < 10; j++)
            {
                GameObject nq = GameObject.Instantiate(qizi, new Vector3(i, j, 0), Quaternion.identity);
                nq.GetComponent<WeiQiZi>().setXY(i, j);
            }
        }
    }

    public void ChangeColor()
    {
        nowBlack = !nowBlack;
    }

    public void ToBlack()
    {
        nowBlack = true;
    }

    public void ToWhite()
    {
        nowBlack = false;
    }

    public void eat1B()
    {

    }

    public void eat1W()
    {

    }

    void CatchHim()
    {
        Collider2D hit = Physics2D.OverlapPoint(Camera.main.ScreenToWorldPoint(Input.mousePosition));
        if (!hit)
            return;
        if (hit.GetComponent<WeiQiZi>())
        {
            if (nowBlack)
                hit.GetComponent<WeiQiZi>().goBlack();
            else
                hit.GetComponent<WeiQiZi>().goWhite();
        }
    }
}