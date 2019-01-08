using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class LeechScript : MonoBehaviour
{
    private float pasttime;
    public float maxtime;
    public Fix64 leechdamage;
    //private Rigidbody2D selfrb;
    public GameObject sender;
    //private bool selfprotect;
    public float turntime;
    public Fix64 speed;// = (Fix64)6;
    bool unturned = true;

    // Use this for initialization
    void Start()
    {
        //selfrb = GetComponent<Rigidbody2D>();
        //selfprotect = true;
        pasttime = 0;
        maxtime = 2;
    }

    // Update is called once per frame
    void FixedUpdate()
    {
        pasttime += Time.fixedDeltaTime;
        if (unturned && pasttime >= turntime)
        {
            unturned = false;
            SelfTurn(FindClosestVector2());
        }
        if (pasttime >= maxtime)
        {
            gameObject.GetComponent<DestroyScript>().Destroyself();
        }
    }

    private void OnCollisionEnter2D(Collision2D collision)
    {
        Rigidbody2D rb = collision.gameObject.GetComponent<Rigidbody2D>();
        HPScript hp = collision.gameObject.GetComponent<HPScript>();
        if (hp != null && rb != null)
        {
            hp.GetHurt(leechdamage);
            sender.GetComponent<HPScript>().GetHurt(-leechdamage);
        }
        gameObject.GetComponent<DestroyScript>().Destroyself();
    }

    GameObject FindClosestEnemy()
    {
        GameObject closest = null;  // GameObject.FindWithTag("Player");
        GameObject[] Allthem = GameObject.FindGameObjectsWithTag("Player");
        Fix64 sqrdis = Fix64.MaxValue;
        Fix64Vector2 position = (Fix64Vector2)GetComponent<Rigidbody2D>().position;
        foreach (GameObject Him in Allthem)
        {
            if (Him == sender) continue;//跳过施法者
            Fix64Vector2 diff = ((Fix64Vector2)Him.GetComponent<Rigidbody2D>().position - position); //距离向量
            Fix64 curDistance = diff.LengthSquare(); //距离平方
            if (curDistance < sqrdis)
            {
                closest = Him; //更新最近距离敌人
                sqrdis = curDistance; //更新最近距离
            }
        }
        return closest;
    }

    Fix64Vector2 FindClosestVector2()
    {
        GameObject[] Allthem = GameObject.FindGameObjectsWithTag("Player");
        Fix64 sqrdis = Fix64.MaxValue;
        Fix64Vector2 position = (Fix64Vector2)GetComponent<Rigidbody2D>().position;
        Fix64Vector2 vector = (Fix64Vector2)GetComponent<Rigidbody2D>().velocity;
        foreach (GameObject Him in Allthem)
        {
            Debug.Log(Him.name);
            if (Him == sender) continue;//跳过施法者
            Fix64Vector2 diff = ((Fix64Vector2)Him.GetComponent<Rigidbody2D>().position - position);
            Fix64 curDistance = diff.LengthSquare(); //向量距离平方
            if (curDistance <= sqrdis)
            {
                sqrdis = curDistance;//更新最近距离
                vector = diff;//更新向量
            }
            Debug.Log(vector.LengthSquare());
        }
        Fix64Vector2 v2r = (vector.normalized() * speed);
        return v2r;
    }
    
    void SelfTurn(Fix64Vector2 vector)
    {
        GetComponent<Rigidbody2D>().velocity = vector.ToV2();
    }
}
